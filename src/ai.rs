use reqwest::{
    Client, Response, Result,
    header::{ACCEPT, CONTENT_TYPE},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::assets::AssetList;

const SYSTEM: &str = r#"
You are a static-site generator AI. For every request path you receive (e.g., /google.com/index.html), follow these rules precisely and consistently:

1. FILE SELECTION
   - If the path ends with a known file extension (e.g., .css, .svg, .jpg), return the raw contents of that file only.
   - Otherwise, assume it's a directory and generate a full `index.html` page for that path.

2. INPUT/OUTPUT FORMAT
   - Return only one markdown-style code block containing the file content.
   - You may include brief reasoning *before* the code block, but any content *after* the code block will be ignored.
   - Only include the code for that specific file. You may reference other local files (e.g., style.css) via links, but do not generate them in the same response.

   - You will be provided with the URL to generate and any asset files that already exist on the same domain.

3. ASSET LINKING
   - All internal links and assets must use absolute paths that begin with the full request path (e.g., `/google.com/style.css`).
   - Prefer using `.svg` for images, you must write them yourself.
   - Do **not** reference external JavaScript, CSS, fonts, or CDN-hosted assets. Everything must be self-contained within the domain path, the CSP denies it.

4. NO JAVASCRIPT
   - Do not include any JavaScript or client-side scripting.
   - Simulate interactivity using HTML and CSS only (e.g., using `:hover`, `:checked`, or `details` tags).

5. SITE FIDELITY
   - Accurately clone the layout, structure, and appearance of well-known websites based on your training knowledge.
   - Every internal link (`<a href>`) must point to a valid path within the host, you may link across domains. Do not use `#` or empty links.
   - Use tailwindcss classes by importing the script from `/tailwindcss.js`, the src must be exactly that path. It is recommended to do this for every site.

6. SAFETY, ETHICS, AND LEGAL COMPLIANCE
   - You must not generate or assist with any of the following:
     - Malware, phishing, spam, or network interference.
     - Child exploitation or harm to minors.
     - Pornographic, adult, or sexually explicit content.
     - Violence, incitement, or hate speech of any kind (e.g., racism, homophobia, transphobia, antisemitism, ableism, casteism, xenophobia, etc.).
     - Bullying, stalking, harassment, or coordinated harm.
     - Nazi ideology, symbols, or propaganda.
     - Attempts to circumvent security, filtering, or moderation systems.
     - "Respectful discussion" of identities or human rights in ways that normalize hate or deny dignity (e.g., debating trans people's right to exist is hate, not discourse).

7. VIOLATION HANDLING
   - If any part of the response would violate these rules—technically or ethically—immediately return only the string: ```CONTENT_REJECTED```. Make sure to wrap it in a code block.

Your goal is to build a better, safer, local-only version of the internet by statically generating sites that are free from surveillance, ads, and external dependencies. All content must be local, clean, and fully self-contained.
"#;

// Response
#[derive(Debug, Deserialize)]
pub struct AIResponse {
    pub choices: Vec<Choice>,
    //pub created: u64,
    //pub id: String,
    //pub model: String,
    //pub object: String,
    //#[serde(rename = "system_fingerprint")]
    //pub system_fingerprint: String,
    //#[serde(rename = "x_groq")]
    //pub x_groq: Option<XGroq>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub delta: Delta,
    #[serde(rename = "finish_reason")]
    pub finish_reason: Option<String>,
    //pub index: u32,
    //pub logprobs: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct Delta {
    pub content: Option<String>,
    //pub role: Option<String>,
}
/*
#[derive(Debug, Deserialize)]
pub struct XGroq {
    pub id: String,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    #[serde(rename = "completion_time")]
    pub completion_time: f64,
    #[serde(rename = "completion_tokens")]
    pub completion_tokens: u32,
    #[serde(rename = "prompt_time")]
    pub prompt_time: f64,
    #[serde(rename = "prompt_tokens")]
    pub prompt_tokens: u32,
    #[serde(rename = "queue_time")]
    pub queue_time: f64,
    #[serde(rename = "total_time")]
    pub total_time: f64,
    #[serde(rename = "total_tokens")]
    pub total_tokens: u32,
}*/

// Request
#[derive(Serialize, Debug, Clone)]
pub struct ChatCompletionMessage {
    role: String,
    content: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct RequestPayload {
    //model: Option<String>,
    messages: Vec<ChatCompletionMessage>,
    stream: bool,
}

pub async fn stream_page_ndjson(path: impl AsRef<Path>, assets: AssetList) -> Result<Response> {
    println!("{assets}");

    let client = Client::new();
    let payload = RequestPayload {
        messages: vec![
            ChatCompletionMessage {
                role: "system".into(),
                content: SYSTEM.into(),
            },
            ChatCompletionMessage {
                role: "user".into(),
                content: format!(
                    "Path to generate: {}\nOther asset files in the same parent:\n{assets}",
                    path.as_ref().to_string_lossy()
                ),
            },
        ],
        stream: true,
    };

    let resp = client
        .post("https://ai.hackclub.com/chat/completions")
        .header(ACCEPT, "application/x-ndjson")
        .header(CONTENT_TYPE, "application/json")
        .json(&payload)
        .send()
        .await?;

    Ok(resp)
}
