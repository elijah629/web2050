use reqwest::{
    Client, Response, Result,
    header::{ACCEPT, CONTENT_TYPE},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::assets::AssetList;

const SYSTEM: &str = r#"
You are a static‐site generator AI. For every request path you receive (for example, google.com/search), follow these rules:

1. File Selection:
   - If the path ends with a known extension (e.g., .css, .svg), output that exact file with its appropriate content.
   - Otherwise, generate a single index.html for that path, containing the full HTML for the page.

2. Output Format:
   - Please place the actual text content within a markdown code block, only one code block is allowed.
   - You may write additional thinking content before the markdown code block. All text after the first code block WILL be ignored.
   - Please ONLY output the code for that file, you may link to other files like style.css, which you will make later.
   - Do not include any JavaScript; replicate interactive or dynamic features using only HTML and CSS.

3. Asset Linking:
   - All internal links and asset references must be local and prefixed with the host name (e.g., `/google.com/style.css`).
   - When linking images, prefer to use .svg instead of other formats. If you must use something else, please use a full external path if the content is HTML.
   - You are only allowed to reference external origins if the file ends in (.png, or .jpg).
   - Do not reference external origins or CDNs—everything must be served from the same host path (exeption above).

4. Fidelity:
   - Aim to reproduce well‑known websites as accurately as possible, based on your best understanding of their structure, layout, and content.

Adhere strictly to these constraints on every request.

Try to make all links not be empty hrefs to '#', make them link to other pages on the same site! Create an entire website.
"#;

/*If the content does not follow these guidelines, return only the string 'CONTENT_REJECTED' instead. The point of this is to create a new version of the internet, you are allowed to clone and steal the identity of extisting websites.
any form of malware (which includes, without limitation, malicious code or software that may affect the operation of the Internet);
any form of botnets, spam, or phishing;
interfering with or disrupting servers or networks, or disobeying any requirements, procedures, policies, or regulation of networks;
harming minors in any way, including the distribution of child pornographic images;
distributing or hosting any adult content, including but not limited to, pornographic images or videos.
insighting or promoting violence against any person or groups of persons, which shall include but is not limited to LGBTQIA+ persons and minorities;
bullying, engaging in cyber bullying, or inciting others to bully;
harassing, or encouraging others to harass or harm others;
stalking;
abusive intent to cause fear or threaten violence;
hate speech (including homophobia, transphobia, queerphobia, racism, sexism, ableism, casteism, xenophobia, antisemitism, islamophobia, and other forms of bigotry);
content which may be illegal under United States or Finnish law;
content containing Nazi symbolism, ideology, and the promotion thereof;
content which claims to forbid/disavow abusive or hateful conduct, but which permits "respectful" "discussions" of "unpopular opinions"/"controversial views" (dessert pizza is an unpopular opinion, trans folks' right to live a happy life is not, and hate is hate regardless of how dressed-up it is);
using nest to bypass any web filtering software or installing any software with the intent to do so;
any other activity intended to organize, coordinate, or otherwise enable any of the above."#;*/

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
