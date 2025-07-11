#![deny(clippy::all)]

use futures_util::TryStreamExt;
use jwalk::WalkDir;
use rocket::fs::FileServer;
use rocket::http::{ContentType, Status};
use rocket::response::content::RawHtml;
use rocket::response::stream::TextStream;
use rocket::tokio::fs::File;
use rocket::tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use rocket::tokio::sync::{Mutex, Notify, mpsc};
use rocket::tokio::{fs, task};
use rocket::{State, tokio};
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

    if url.as_os_str().len() > 64 {
        return Err(Status::UriTooLong);
    }

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

    let (tx, mut rx) = mpsc::channel::<String>(32);

    tokio::spawn(async move {
        let mut writer = BufWriter::new(file);

        let _guard = guard; // Pull the guard into this scope, when the stream ends, the guard gets
        // dropped;

        let mut in_code_block = false;
        let mut tag_buffer = String::new();
        const MAX_TAG_LEN: usize = 7; // Length of "</code>"

        while let Ok(Some(item)) = lines.next_line().await {
            if let Ok(json) = serde_json::from_str::<AIResponse>(&item) {
                let choice = &json.choices[0];
                if choice.finish_reason.is_some() {
                    writer.flush().await.unwrap();
                    break;
                }
                let delta = choice
                    .delta
                    .content
                    .as_ref()
                    .expect("no delta but response has not finished streaming");

                // Combine previous buffer with new delta
                tag_buffer.push_str(delta);

                if !in_code_block {
                    // Look for opening tag
                    // This minimizes buffer size and is not nessicary for function
                    // setting in_code_block is nessicary
                    if let Some(pos) = tag_buffer.find("<code>") {
                        in_code_block = true;
                        // Remove everything up to and including the tag
                        tag_buffer.drain(..pos + 6);
                    } else {
                        // Keep only the tail that might contain partial tag
                        if tag_buffer.len() > MAX_TAG_LEN {
                            tag_buffer.drain(..tag_buffer.len() - MAX_TAG_LEN);
                        }
                    }
                } else {
                    // Look for closing tag
                    if let Some(pos) = tag_buffer.find("</code>") {
                        // Send everything before the closing tag
                        if pos > 0 {
                            let content = &tag_buffer[..pos];
                            let _ = writer.write_all(content.as_bytes()).await;
                            let _ = tx.send(content.to_string()).await;
                        }
                        in_code_block = false;
                        // Remove everything up to and including the tag
                        tag_buffer.drain(..pos + 7);
                    } else {
                        // Send all but the tail (which might contain partial closing tag)
                        if tag_buffer.len() > MAX_TAG_LEN {
                            let send_len = tag_buffer.len() - MAX_TAG_LEN;

                            let content = &tag_buffer[..send_len];
                            let _ = writer.write_all(content.as_bytes()).await;
                            let _ = tx.send(content.to_string()).await;

                            tag_buffer.drain(..send_len);
                        }
                    }
                }
            }
        }
    });

    let stream = TextStream! {
        while let Some(delta) = rx.recv().await {
            yield delta;
        }
    };

    // Must default to HTML because .com is technically an extension
    let content_type = ContentType::from_extension(&extension).unwrap_or(ContentType::HTML);

    Ok((content_type, stream))
}

#[get("/")]
pub fn index() -> RawHtml<TextStream![String]> {
    RawHtml(TextStream! {
        yield r#"<!DOCTYPE html><html lang="en" class="dark"><head><meta charset="UTF-8"/><meta name="viewport" content="width=device-width,initial-scale=1.0"/><title>web2050</title><link rel="stylesheet" href="/style.css"></head><body class="bg-gray-950 text-gray-100 min-h-screen flex items-center justify-center px-4 py-8"><main class="w-full max-w-2xl"><header class="mb-8 text-center"><h1 class="text-4xl font-bold text-blue-500">web2050 Index</h1><p class="text-gray-400 mt-2"Append any url minus the protocol (https://) to the end of this url (e.hackclub.app) and watch an AI magically generate it in real time!></p><p class="text-gray-400 mt-2">Search the index of all available AI-generated pages.</p></header><section class="mb-6"><input type="text" id="search-input" placeholder="Search..." class="w-full p-3 rounded-lg border border-gray-700 bg-gray-800 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-blue-500"/></section><ul id="index-list" class="space-y-2">"#.to_string();

        for entry in WalkDir::new("internet").into_iter().flatten().skip(1) {
            if let Ok(path) = entry.path().strip_prefix("internet") {
                let path = path.to_string_lossy();
                yield format!(r#"<li class="index-item" data-path="{0}"><a href="/{0}" class="block p-3 rounded-md bg-gray-800 hover:bg-gray-700 text-blue-500 transition-colors">{0}</a></li>"#, path);
            }
        }

        yield r#"</ul></main><script>document.addEventListener("DOMContentLoaded",()=>{const e=document.getElementById("search-input"),t=document.querySelectorAll(".index-item");e.addEventListener("input",()=>{const n=e.value.toLowerCase();t.forEach(e=>{const t=e.getAttribute("data-path").toLowerCase();e.style.display=t.includes(n)?"":"none"})})})</script></body></html>"#.to_string();
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
