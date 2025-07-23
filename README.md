# web2050

> The AI-Generated web.

Append any url to the base url (after /, minus protocol) and it will live-stream generate the specific page!. After, your browser uses it again to recursively generate a site, including all assets, pages, and js! Uses [https://ai.hackclub.com](https://ai.hackclub.com) internally.

## Why?

To show that 2-shot vibecoding a webpage is not a good idea 💀

## Security

The CSP disallows all external assets, and the AI has been prompted to follow Hackclub Nest's CoC.

## Demo

Demo available [here](https://ai.dino.icu)

## Self-Hosting

- Set `.env`
  ```env
  HOST=0.0.0.0:port
  ```

- Run 
  ```sh
  cargo run --release
  ```

- Available at `$HOST`
