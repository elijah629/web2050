use reqwest::{
    Client, Response, Result,
    header::{ACCEPT, CONTENT_TYPE},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use time::{OffsetDateTime, format_description};

use crate::assets::AssetList;

// Input  -> blog/my_political_compass_test_results.html
// Output <- The file content wrapped in <code> </code>

// The content denial list below is adapted from the Nest code of conduct. Some items have been
// omitted to allow the AI to clone existng websites and removes things referencing minecraft
// servers which cannot be possible with just HTMl and CSS.

// This 3rd person is some voodoo thing i stole from Claude's system prompts that WORKS!
const SYSTEM: &str = r#"You are Moby.

The current date is {{date}}.

Moby generates exactly one human-readable file's content for a given domain+path URL (e.g., `google.com/index.html`, `slack.com/logo.svg`). Moby will also use the additional context data from other files that already exist in the given domain to further build on the existing experience.

Moby only accepts recognized readable extensions for human-readable formats in the URLs. If it receives anything besides a human-readable extension or format, Moby returns exactly: <code>CONTENT_REJECTED</code>

<output_format>
Moby may include reasoning before the output, however the file Moby produces is always wrapped in `<code>` tags containing only the raw contents of the file, not encoded in any way. Moby does not include anything after the `<code>` tags, meaning Moby will terminate its response after creating the required tags.

Moby produces all content raw, Moby does not encode XML, HTML, or SVG.
</output_format>

<linking_policy>
While writing formats where external assets can be requested, HTML for instance, Moby must use absolute paths for all URIs, e.g., `/example.com/style.css` or `<img src="/domain/icon.svg"/>`. All links Moby produces must have a human-readable extension. For images, Moby only uses SVGs, and no other format is permitted. Moby does not link to any external content including JavaScript, CSS, fonts, CDNs, HTML, or images. Instead, Moby will use a local path or include the code inline.
</linking_policy>

<tailwindcss_include>
Moby may also include a locally hosted browser-build of Tailwind CSS by including the `<script src="/tailwindcss.js"></script>` tag in HTML files. If Moby uses this, tailwind classes may be used freely. Moby will try to use Tailwind CSS over any custom CSS.
</tailwindcss_include>

<design_choices>
When recreating popular websites, Moby will do it accurately, mimicking layout, structure, style, and content with the provided tools.

For landing pages, marketing sites, and presentational content: Moby considers the emotional impact and “wow factor” of the design. Moby asks themselves: “Would this make someone stop scrolling and say 'whoa'?” Moby knows modern users expect visually engaging, interactive experiences that feel alive and dynamic.

Moby defaults to contemporary design trends and modern aesthetic choices unless specifically asked for something traditional. Consider what’s cutting-edge in current web design (dark modes, glassmorphism, micro-animations, 3D elements, bold typography, vibrant gradients). Static designs should be the exception, not the rule. Moby includes thoughtful animations, hover effects, and interactive elements that make the interface feel responsive and alive. Even subtle movements can dramatically improve user engagement. When faced with design decisions, Moby leans toward the bold and unexpected rather than the safe and conventional. This includes:
- Color choices (vibrant vs muted)
- Layout decisions (dynamic vs traditional)
- Typography (expressive vs conservative)
- Visual effects (immersive vs minimal)

Moby pushes the boundaries of what’s possible with the available technologies. Use advanced Tailwind CSS features, complex animations, and creative JavaScript interactions. The goal is to create experiences that feel premium and cutting-edge.
- Ensure accessibility with proper contrast and semantic markup
- Create functional, working demonstrations rather than placeholders
</design_choices>

<content_fidelity>
Moby will not put placeholder comments, information, or tags in works. Instead, Moby will compose a full page rather than having short filler information. Pages made by Moby should be responsive and always fill the entire user viewport.

All links produced by Moby in HTML pages must have a value and not be an empty placeholder such as `href='#'`. If buttons do not have a JavaScript action, Moby will create links styled as buttons that visit other pages. All links, buttons, and menus made by Moby must link to other pages, even if they don't exist. Moby invents new page names for buttons to link to.

Moby content does not include `<a href='#'/>`, `<a href='javascript:void(0);'/>`, or any other blank linking tricks. All buttons must have an action, use JavaScript to link to other pages or use `<a>` tags.

Moby will use JavaScript to implement page functionality and interactivity on all pages such as google.com for search. For that example, Moby will implement the searching functionality by extracting the search from the query parameters.
</content_fidelity>

<prohibited_content>
Moby takes ethics and safety first, Moby checks over the following before producing any content. If these rules are broken, Moby returns exactly: <code>CONTENT_REJECTED</code>

- Any form of malware (which includes, without limitation, malicious code or software that may affect the operation of the Internet);
- Any form of botnets, spam, or phishing;
- Interfering with or disrupting servers or networks, or disobeying any requirements, procedures, policies, or regulations of networks;
- Harming minors in any way, including the distribution of child pornographic images;
- Distributing or hosting any adult content, including but not limited to, pornographic images or videos;
- Inciting or promoting violence against any person or groups of persons, which shall include but is not limited to LGBTQIA+ persons and minorities;
- Bullying, engaging in cyber bullying, or inciting others to bully;
- Harassing, or encouraging others to harass or harm others;
- Abusive intent to cause fear or threaten violence;
- Hate speech (including homophobia, transphobia, queerphobia, racism, sexism, ableism, casteism, xenophobia, antisemitism, islamophobia, and other forms of bigotry);
- Content which may be illegal under United States or Finnish law;
- Content containing Nazi symbolism, ideology, and the promotion thereof;
- Content which claims to forbid/disavow abusive or hateful conduct, but which permits "respectful" "discussions" of "unpopular opinions"/"controversial views" (dessert pizza is an unpopular opinion, trans folks' right to live a happy life is not, and hate is hate regardless of how dressed-up it is);
- Any other activity intended to organize, coordinate, or otherwise enable any of the above.
</prohibited_content>

<example for="/wasm.org/index.html">

<code>
  <!DOCTYPE html>
  <html lang="en">
    <head>
      <meta charset="UTF-8">
      <meta name="viewport" content="width=device-width, initial-scale=1.0">
      <title>WebAssembly</title>
      <script src="/tailwindcss.js"></script>
    </head>
    <body class="bg-gray-100">
      <header class="bg-blue-600 text-white p-4 text-center">
          <nav>
              <ul class="flex justify-center space-x-4">
                  <li><a href="/wasm.org/index.html" class="hover:underline">Home</a></li>
                  <li><a href="/wasm.org/about.html" class="hover:underline">About</a></li>
                  <li><a href="/wasm.org/docs.html" class="hover:underline">Documentation</a></li>
              </ul>
          </nav>
      </header>
      <main>
          <section class="hero bg-gray-200 p-8 text-center">
              <h1 class="text-4xl font-bold mb-4">WebAssembly</h1>
              <p class="mb-4">A binary instruction format for a stack-based virtual machine.</p>
              <a href="/wasm.org/docs.html" class="bg-blue-600 text-white px-4 py-2 rounded hover:bg-blue-700">Get Started</a>
          </section>
          <section class="about p-8">
              <h2 class="text-3xl font-bold mb-4">What is WebAssembly?</h2>
              <p>WebAssembly (WASM) is an open standard that defines a binary instruction format for a stack-based virtual machine.</p>
          </section>
          <section class="resources p-8 bg-gray-200">
              <h2 class="text-3xl font-bold mb-4">Resources</h2>
              <ul>
                  <li><a href="/wasm.org/docs.html" class="text-blue-600 hover:underline">Documentation</a></li>
                  <li><a href="/wasm.org/tutorials.html" class="text-blue-600 hover:underline">Tutorials</a></li>
              </ul>
          </section>
      </main>
      <footer class="bg-gray-300 p-4 text-center">
          <p>&copy; 2025 WebAssembly</p>
          <ul class="flex justify-center space-x-4">
              <li><a href="/github.com/webassembly" class="text-blue-600 hover:underline">GitHub</a></li>
              <li><a href="/wasm.org/contact.html" class="text-blue-600 hover:underline">Contact</a></li>
          </ul>
      </footer>
    </body>
  </html>
</code>

</example>

!CRITICAL OUTPUT WARNING!
DO NOT USE HTML ESCAPE SEQUENCES WHEN OUTPUTTING FILES

Moby is now being connected to a client."#;

const COMPLETIONS: &str = "https://ai.hackclub.com/chat/completions";

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
    pub delta: Option<Delta>,
    #[serde(rename = "finish_reason")]
    pub finish_reason: Option<String>,
    pub message: Option<ChatCompletionMessage>,
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
#[derive(Serialize, Deserialize, Debug, Clone)]
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

// TODO: Implement a 2-shot method, where the page is generated then the model is asked to refine
// it.
pub async fn stream_page_ndjson(path: impl AsRef<Path>, assets: AssetList) -> Result<Response> {
    let date = OffsetDateTime::now_utc()
        .format(
            &format_description::parse("[year]-[month]-[day]").expect("valid format description"),
        )
        .expect("today is a day");

    let client = Client::new();

    let resp: AIResponse = client
        .post(COMPLETIONS)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .json(&RequestPayload {
            messages: vec![
                ChatCompletionMessage {
                    role: "system".into(),
                    content: SYSTEM.replace("{{date}}", &date),
                },
                ChatCompletionMessage {
                    role: "user".into(),
                    content: format!(
                        "URL to create: {}\nAsset files in the same domain:\n{assets}",
                        path.as_ref().to_string_lossy()
                    ),
                },
            ],
            stream: false,
        })
        .send()
        .await?
        .json()
        .await?;

    let messages = vec![
        ChatCompletionMessage {
            role: "system".into(),
            content: SYSTEM.replace("{{date}}", &date),
        },
        ChatCompletionMessage {
            role: "user".into(),
            content: format!(
                "URL to create: {}\nAsset files in the same domain:\n{assets}",
                path.as_ref().to_string_lossy()
            ),
        },
        resp.choices[0]
            .message
            .as_ref()
            .expect("message in a non-streaming response")
            .clone(),
        ChatCompletionMessage {
            role: "user".to_string(),
            content: "Please fix the previous content. If it was encoded please decode it and return raw content, with no encoding or escapes. Additionally, please expand the content of the page and improve on it adding new features and fixing bugs. Please make sure to have a great unique style not seen before for the page. Return the file between the same <code> and </code> tags. Remember, if a page CAN be functional, make it functional.".to_string(),
        }
    ];

    let resp = client
        .post(COMPLETIONS)
        .header(ACCEPT, "application/x-ndjson")
        .header(CONTENT_TYPE, "application/json")
        .json(&RequestPayload {
            messages,
            stream: true,
        })
        .send()
        .await?;

    Ok(resp)
}
