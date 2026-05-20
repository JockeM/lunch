# lunch

Small Rust CLI that fetches today's lunch menus from the configured restaurant
sources and renders them either as plain text or as a Slack incoming-webhook
payload.

## Usage

```sh
cargo run -- today
cargo run -- slack
```

`today` is the default command, so `cargo run` prints the same output as
`cargo run -- today`.

## Hosting

The project is deployed as a compiled binary on a small Linux host.
