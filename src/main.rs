#![deny(clippy::all)]
use async_stream::stream;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::handler::HandlerWithoutStateExt;
use axum::http::{HeaderValue, Request, Response, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Router, middleware};
use futures_util::TryStreamExt;
use mime_guess::Mime;
use std::collections::HashMap;
use std::env::current_dir;
use std::ffi::OsString;
use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::sync::Arc;
use tokio::fs::{self, File};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::{Mutex, Notify, mpsc};
use tokio_util::io::StreamReader;
use tower_http::services::ServeDir;

use crate::ai::AIResponse;
mod ai;
mod assets;

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

        tokio::task::spawn(async move {
            let mut guard = map.lock().await;
            guard.remove(&key);
            notifier.notify_waiters();
        });
    }
}

async fn generate(
    url: Uri,
    State(gen_map): State<GenerationMap>,
) -> Result<impl IntoResponse, StatusCode> {
    use std::path::Path;

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

    let url = url.path();
    let url = url.strip_prefix('/').unwrap_or(url);
    let url = PathBuf::from(url);

    let extension = url.extension().and_then(|x| x.to_str());

    if let Some("map") = extension {
        return Err(StatusCode::BAD_REQUEST);
    }

    let (url, extension) = match (url.components().count(), extension) {
        (1, _) | (_, None) => (url.join("index.html"), "html"),
        (_, Some(ext)) => (url.clone(), ext),
    };

    if url.as_os_str().len() > 72 {
        return Err(StatusCode::URI_TOO_LONG);
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
    fs::create_dir_all(&parent_fs_path).await.map_err(|e| {
        eprintln!("{e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Fetch all assets relating to the domain. We should wait, but it is unlikely that a request
    // to a/b happens while a request to a is already happening.
    //
    // This gives the AI WAAAY more context though. We could not include content for files not in
    // the current route, and only include an abstract tree..
    let assets = assets::read_all_files_in_dir(&fs_domain)
        .await
        .map_err(|e| {
            eprintln!("{e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let resp = ai::stream_page_ndjson(&url, assets).await.map_err(|e| {
        eprintln!("{e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let stream = resp.bytes_stream();
    let stream_reader = StreamReader::new(stream.map_err(std::io::Error::other));

    let mut lines = BufReader::new(stream_reader).lines();

    // RAII guard that, on drop, removes the key from the map and notifies that the req is
    // done.
    let guard = GenerationGuard {
        key: key.clone(),
        map: gen_map.clone(),
        notifier: notifier.clone(),
    };

    let file = File::create(Path::new("internet").join(url))
        .await
        .map_err(|e| {
            eprintln!("{e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

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
                    let _ = writer.flush().await;
                    break;
                }
                let delta = choice
                    .delta
                    .as_ref()
                    .expect("delta since streaming")
                    .content
                    .as_ref()
                    .expect("no delta but response has not finished streaming");

                // Combine previous buffer with new delta
                tag_buffer.push_str(delta);

                if !in_code_block {
                    // Look for opening tag
                    if let Some(pos) = tag_buffer.find("<code>") {
                        in_code_block = true;
                        // Remove everything up to and including the tag
                        tag_buffer.drain(..pos + 6);
                    } else {
                        // Keep only the tail that might contain partial tag, respecting UTF-8 boundaries
                        if tag_buffer.len() > MAX_TAG_LEN {
                            let keep_from =
                                find_utf8_boundary(&tag_buffer, tag_buffer.len() - MAX_TAG_LEN);
                            tag_buffer.drain(..keep_from);
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
                        // Remove everything up to and including the tag
                        tag_buffer.drain(..pos + 7);
                        let _ = writer.flush().await;
                        break;
                    } else {
                        // Send all but the tail (which might contain partial closing tag)
                        if tag_buffer.len() > MAX_TAG_LEN {
                            let send_len =
                                find_utf8_boundary(&tag_buffer, tag_buffer.len() - MAX_TAG_LEN);
                            if send_len > 0 {
                                let content = &tag_buffer[..send_len];
                                let _ = writer.write_all(content.as_bytes()).await;
                                let _ = tx.send(content.to_string()).await;
                                tag_buffer.drain(..send_len);
                            }
                        }
                    }
                }
            }
        }
    });

    let stream = stream! {
        while let Some(delta) = rx.recv().await {
            yield Ok::<String, std::convert::Infallible>(delta);
        }
    };

    // Must default to HTML because .com is technically an extension
    let mime_type = mime_guess::from_ext(extension).first_or(Mime::from_str("text/html").unwrap());
    //.unwrap_or(ContentType::HTML);

    Ok(Response::builder()
        .header("Content-Type", mime_type.as_ref())
        .body(Body::from_stream(stream))
        .unwrap())
}

fn find_utf8_boundary(s: &str, mut pos: usize) -> usize {
    // If pos is beyond string length, return string length
    if pos >= s.len() {
        return s.len();
    }

    // Walk backwards to find a valid UTF-8 character boundary
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

async fn index(Query(params): Query<HashMap<String, String>>) -> Result<Body, StatusCode> {
    let cwd = current_dir()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .join("internet");

    let query = params.get("q");
    let content = query.is_some();

    let mut rg = Command::new("rg");

    let mut rg = if let Some(term) = query {
        rg.arg("-i").arg(term)
    } else {
        rg.arg("--files")
    }
    .arg("--no-ignore-vcs")
    .arg("--sortr=created") // New at top
    .current_dir(cwd)
    .stdout(Stdio::piped())
    .spawn()
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let stream = stream! {
        // Head
        yield Ok::<_, std::convert::Infallible>(r#"<!DOCTYPE html>
<html lang="en" class="dark">
<head>
  <meta charset="UTF-8"/>
  <meta name="viewport" content="width=device-width,initial-scale=1.0"/>
  <title>web2050</title>
  <link rel="stylesheet" href="/style.css">
</head>
<body class="bg-gray-950 text-gray-100 min-h-screen flex items-center justify-center px-4 py-8">
  <main class="w-full max-w-2xl">
    <header class="mb-8 text-center">
      <h1 class="text-4xl font-bold text-blue-500">web2050 Index</h1>
      <p class="text-gray-400 mt-2">Append any URL minus the protocol (https://) to the end of this URL and watch AI generate it in real time.</p>
      <p class="text-gray-400 mt-2">Search the index of all AI-generated pages sorted descending by creation. <span id="counter">0</span> pages have been generated so far.</p>
    </header>
    <section class="mb-6 w-full flex space-x-2">
      <form method="get" class="flex w-full">
        <input
          type="text"
          name="q"
          id="search-input"
          value=""
          placeholder="Search by term or path..."
          class="flex-1 p-3 rounded-l-lg border border-gray-700 bg-gray-800 text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
        <button type="submit" class="p-3 bg-blue-500 rounded-r-lg text-white hover:bg-blue-600 focus:ring-2 focus:ring-blue-400">
          Search by content
        </button>
      </form>
    </section>
    <ul id="list" class="space-y-2">"#.to_string());

    yield Ok(r#"<script>
  const ul = document.getElementById("list");
  const counter = document.getElementById("counter");

  // counter.textContent = ul.querySelectorAll("li").length;

  const observer = new MutationObserver((mutationsList) => {
    let newItems = 0;
    for (const mutation of mutationsList) {
      if (mutation.type === 'childList') {
        newItems += mutation.addedNodes.length;
      }
    }
    if (newItems > 0) {
      counter.textContent = ul.querySelectorAll("li").length;
    }
  });

  observer.observe(ul, { childList: true });
</script>"#.to_string());

    if let Some(stdout) = rg.stdout.take() {
        let reader = std::io::BufReader::new(stdout);

        for line in reader.lines().map_while(Result::ok) {
            if content {
                if let Some((path, snippet)) = line.split_once(':') {
                    yield Ok(format!(
                        r#"<li class="p-3 rounded-md bg-gray-800 hover:bg-gray-700 transition-colors" data-path="{0}">
  <a href="/{0}" class="text-blue-500">{0}</a>
  <pre class="whitespace-pre-wrap break-words text-gray-100 bg-gray-800 p-2 rounded-md mt-2"><code>{1}</code></pre>
</li>"#,
                        path,
                        html_escape::encode_text(snippet)
                    ));
                }
            } else {
                yield Ok(format!(
                    r#"<li class="p-3 rounded-md bg-gray-800 hover:bg-gray-700 transition-colors" data-path="{0}">
  <a href="/{0}" class="text-blue-500">{0}</a>
</li>"#,
                    line
                ));
            }
        }
    }

        // Footer and script
        yield Ok(r##"</ul>
  </main>
  <script>
    // Live filtering on input
    document.addEventListener("DOMContentLoaded", () => {
      const input = document.getElementById("search-input");
      const items = document.querySelectorAll("li");
      input.addEventListener("input", () => {
        const q = input.value.toLowerCase();
        items.forEach(el => {
          const path = el.getAttribute("data-path");
          el.style.display = path && path.toLowerCase().includes(q) ? "block" : "none";
        });
      });
    });
  </script>
</body>
</html>"##.to_string());
    };

    Ok(Body::from_stream(stream))
}

async fn csp(req: Request<Body>, next: Next) -> Response<Body> {
    let mut response = next.run(req).await;

    let csp = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src *; font-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none';";
    response.headers_mut().insert(
        "Content-Security-Policy",
        HeaderValue::from_str(csp).unwrap(),
    );

    response
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use dotenvy::EnvLoader;

    let env = EnvLoader::new().load()?;
    let gen_map: GenerationMap = Arc::new(Mutex::new(HashMap::new()));

    let service = get(generate).with_state(gen_map).into_service();

    let app = Router::new()
        .route("/", get(index))
        .fallback_service(ServeDir::new("internet").fallback(service))
        .layer(middleware::from_fn(csp));

    let listener = tokio::net::TcpListener::bind(env.var("HOST")?)
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();

    Ok(())

    /*rocket::build()
    .manage(gen_map)
    .attach(csp::CSPFairing)
    .mount("/", routes![index, generate])
    .mount("/", FileServer::from("internet").rank(3))*/
}
