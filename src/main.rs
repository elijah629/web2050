#![deny(clippy::all)]

use futures_util::TryStreamExt;
use jwalk::WalkDir;
use rocket::State;
use rocket::fs::FileServer;
use rocket::http::{ContentType, Status};
use rocket::response::content::RawHtml;
use rocket::response::stream::TextStream;
use rocket::tokio::fs::File;
use rocket::tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use rocket::tokio::sync::{Mutex, Notify};
use rocket::tokio::{fs, task};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_util::io::StreamReader;

use crate::ai::AIResponse;

mod ai;
mod assets;
mod csp;

#[macro_use]
extern crate rocket;

type GenerationMap = Arc<Mutex<HashMap<OsString, Arc<Notify>>>>;

struct GenerationGuard {
    key: OsString,
    map: Arc<Mutex<HashMap<OsString, Arc<Notify>>>>,
    notifier: Arc<Notify>,
}

impl Drop for GenerationGuard {
    fn drop(&mut self) {
        let key = self.key.clone();
        let map = self.map.clone();
        let notifier = self.notifier.clone();

        task::spawn(async move {
            let mut guard = map.lock().await;
            guard.remove(&key);
            notifier.notify_waiters();
        });
    }
}

#[get("/<url..>", rank = 2)]
async fn generate(
    url: PathBuf,
    gen_map: &State<GenerationMap>,
) -> Result<(ContentType, TextStream![String]), Status> {
    // Generates the path
    //
    // For generation need to check the global mutex:
    // If any files in the current route are currently being generated, we will hang the response
    // until the generation for the sibling assets are done. This will be handled in order of
    // request.
    //
    // Example:
    // REQ /google.com/index.html - Not cached, start streaming and begin generation
    // REQ /google.com/style.css  - This might come from the same user who requested index.html, as
    // browsers will still fetch <link> and <script> even if the content is still half-baked. This
    // request should HANG until index.html is done
    // REQ /google.com/assets/icon.svg - Hang like above, since this req is on the same common route, /

    let (url, extension) = match (url.components().count(), url.extension()) {
        (1, _) | (_, None) => (url.join("index.html"), "html".into()),
        (_, Some(ext)) => (url.clone(), ext.to_string_lossy()),
    };

    if let Cow::Borrowed("map") = extension {
        return Err(Status::BadRequest);
    }

    let key = url
        .parent()
        .expect("cannot reach generator with no parent")
        .as_os_str()
        .to_os_string();

    let (we_are_the_inserter, notifier) = {
        let mut map = gen_map.lock().await;

        match map.get(&key) {
            Some(existing_notifier) => (false, existing_notifier.clone()),
            None => {
                let new_notifier = Arc::new(Notify::new());
                map.insert(key.clone(), new_notifier.clone());
                (true, new_notifier)
            }
        }
    };

    if !we_are_the_inserter {
        notifier.notified().await;
    }

    println!("GEN {url:?}");

    // Regardless of whether the current folder was being generated on, we must still generate
    // this one.

    let fs_path = Path::new("internet").join(&url);
    let fs_domain = Path::new("internet").join(url.iter().next().expect("URL must have a domain"));
    let parent_fs_path = fs_path
        .parent()
        .expect("cannot reach generator with empty path");

    // Create all the folders
    fs::create_dir_all(&parent_fs_path)
        .await
        .map_err(|_| Status::InternalServerError)?;

    // Fetch all assets relating to the domain. We should wait, but it is unlikely that a request
    // to a/b happens while a request to a is already happening.
    //
    // This gives the AI WAAAY more context though. We could not include content for files not in
    // the current route, and only include an abstract tree..
    let assets = assets::read_all_files_in_dir(&fs_domain)
        .await
        .map_err(|_| Status::InternalServerError)?;

    let resp = ai::stream_page_ndjson(&url, assets)
        .await
        .map_err(|_| Status::InternalServerError)?;

    let stream = resp.bytes_stream();
    let stream_reader = StreamReader::new(stream.map_err(std::io::Error::other));

    let mut lines = BufReader::new(stream_reader).lines();

    let mut in_code_block = false;
    let mut code_block_started = false;
    let mut code_block_ended = false;
    let mut buffer = String::new();
    let mut skip_until_newline = false;

    // RAII guard that, on drop, removes the key from the map and notifies that the req is
    // done.
    let guard = GenerationGuard {
        key: key.clone(),
        map: gen_map.inner().clone(),
        notifier: notifier.clone(),
    };

    let file = File::create(Path::new("internet").join(url))
        .await
        .map_err(|_| Status::InternalServerError)?;
    let mut writer = BufWriter::new(file);

    let stream = TextStream! {
        let _guard = guard; // Pull the guard into this scope, when the stream ends, the guard gets
        // dropped;

        while let Ok(Some(item)) = lines.next_line().await {
            if let Ok(json) = serde_json::from_str::<AIResponse>(&item) {
                let choice = &json.choices[0];

                if choice.finish_reason.is_some() {
                    writer.flush().await.unwrap();
                    break;
                }

                let delta = choice.delta.content.as_ref().expect("no delta but response has not finished streaming");

                if code_block_ended {
                    continue;
                }

                buffer.push_str(delta);

                while let Some(pos) = buffer.find("```") {
                    if !code_block_started {
                        // Found the opening delimiter
                        in_code_block = true;
                        code_block_started = true;
                        skip_until_newline = true;
                        buffer.drain(..pos + 3); // remove up to and including the delimiter
                    } else if in_code_block {
                        // Found the closing delimiter
                        in_code_block = false;
                        code_block_ended = true;
                        let content = buffer[..pos].to_string();
                        for line in content.lines() {
                            let line = format!("{line}\n");
                            writer.write_all(line.as_bytes()).await.unwrap(); // TODO: Error the
                            // entire request
                            yield line;
                        }
                        break;
                    } else {
                        break;
                    }
                }

                if in_code_block && !code_block_ended {
                    if skip_until_newline {
                        if let Some(pos) = buffer.find('\n') {
                            buffer.drain(..=pos);
                            skip_until_newline = false;
                        } else {
                            // Wait for full newline
                            continue;
                        }
                    }

                    let lines: Vec<&str> = buffer.lines().collect();
                    let mut line_start = 0;
                    for (i, line) in lines.iter().enumerate() {
                        if i < lines.len() - 1 {
                            let line = format!("{line}\n");
                            writer.write_all(line.as_bytes()).await.unwrap(); // TODO: Error the
                            // entire request
                            line_start += line.len();
                            yield line;
                        }
                    }
                    buffer.drain(..line_start);
                }
            }
        }

        // Drop guard
    };

    // Must default to HTML because .com is technically an extension
    let content_type = ContentType::from_extension(&extension).unwrap_or(ContentType::HTML);

    Ok((content_type, stream))
}

#[get("/", rank = 0)]
fn index() -> RawHtml<TextStream![String]> {
    RawHtml(TextStream! {
        yield format!("<!DOCTYPE HTML><html><body><h2>Welcome to web2025! What follows is the index of the entire site, this is streamed and may take a while to completely generate...</h2>");

        yield "<ul>".to_string();

        for entry in WalkDir::new("internet").into_iter().flatten().skip(1) {
            if let Ok(path) = entry.path().strip_prefix("internet") {
                let path = path.to_string_lossy();
                yield format!("<li><a href='/{path}'>{path}</a></li>");
            }
        }

        yield "</ul></body></html>".to_string();
    })
}

#[launch]
fn rocket() -> _ {
    let gen_map: GenerationMap = Arc::new(Mutex::new(HashMap::new()));

    rocket::build()
        .manage(gen_map)
        .attach(csp::CSPFairing)
        .mount("/", FileServer::from("internet").rank(1))
        .mount("/", routes![index, generate])
}
