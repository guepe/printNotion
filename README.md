# printNotion

Print a Notion page to a clean A4 PDF that looks like Notion, in one click.

## Why

I wanted a simple way to print my Notion pages, like a weekly meal plan or a procedure, without
going through *Export → PDF → open → fix the layout → print* every time. Notion's own export
doesn't handle dashboard-style pages well: linked databases, columns, calendars, and filtered views
come out broken or empty.

I looked for a tool that does this properly and didn't find one, so I built it.

## Features

- **Paste a link, get a PDF.** A small window pre-fills the Notion link from your clipboard,
  generates the PDF in your Downloads folder, and opens it. A command-line mode is also available.
- **Looks like Notion.** The PDF keeps:
  - the page cover, the page icon, and color emojis;
  - colored select and status tags;
  - the page's column layout.
- **Databases are printed the way you see them.** Each embedded database follows its first view:
  - same filters (including quick filters) and sort order;
  - same visible columns, in the same order and with the same widths;
  - same grouping;
  - calendar views as a week or month grid, and gallery views as cards.
- **Made for paper.**
  - A4, switching to landscape automatically when needed.
  - A header with the page title, and a footer with the print date and "page 2/5".
  - Small tables are never split across pages.
  - Archive toggles (such as "old") are skipped.
- **Print directly** with `--print` or `--printer NAME`. This uses CUPS on macOS and Linux.
- **Fast.** API calls run in parallel, and database rows are fetched in batches. A page with
  6 databases takes about 7 seconds.

## Installation (macOS, Apple Silicon)

1. Download `printNotion-1.0.0-macos-arm64.zip` from the
   [latest release](https://github.com/guepe/printNotion/releases/latest) and unzip it.
2. The binary is not signed. The first time you open it, right-click it and choose *Open*, or run:
   ```sh
   xattr -d com.apple.quarantine printNotion
   ```

## Setup: connect to Notion

1. Create an **internal integration** at <https://www.notion.so/profile/integrations> and copy its
   secret (it starts with `ntn_`).
2. Give printNotion the token in one of these two ways:
   - set an environment variable: `export NOTION_TOKEN=ntn_...`
   - or create the file `~/Library/Application Support/printnotion/config.toml` containing:
     ```toml
     token = "ntn_..."
     ```
3. **Share your pages with the integration.** In Notion, open the page and go to *••• → Connections*,
   then add your integration. Linked databases also need their **source database** to be shared.
   If one isn't, printNotion tells you, and the app offers a *Give access* button.

## Usage

**App:** run `printNotion` with no arguments, paste a Notion link, and click *Generate PDF*.

**Command line:**

```sh
printNotion "https://www.notion.so/My-page-1a2b3c4d…"   # writes "<page title>.pdf" in the current folder
printNotion <url> -o ~/Desktop/                          # choose an output file or folder
printNotion <url> --print                                # send to the default printer
printNotion <url> --printer "Office_Printer"             # send to a specific printer
```

**Tips:**

- printNotion prints the **first tab** of each database. To print another view, drag its tab to
  the first position in Notion.
- Toggles named `old`, `archive`, `archives`, `ancien`, or `anciens` are skipped. You can change
  this list in `config.toml`:
  ```toml
  ignore_toggles = ["old", "drafts"]
  ```

## Known limitations

- **macOS only for now.** The Notion-like look relies on macOS fonts: Avenir Next, Apple Color
  Emoji, and Hiragino for Japanese text. A Windows build is possible but hasn't been done.
- **What the Notion API doesn't provide:**
  - the manual order of groups (groups follow the order of the database options);
  - which tab you are currently looking at;
  - whether a toggle is collapsed or expanded.
- **Not supported yet:** images, simple tables, and callouts inside the page content; the content
  of sub-pages (they are printed as links).

## Building from source

```sh
cargo build --release   # requires a recent stable Rust (tested with 1.99)
cargo test
```

Built with [Typst](https://typst.app) for PDF layout, [egui](https://github.com/emilk/egui) for the
window, and the [Notion API](https://developers.notion.com) (version `2026-03-11`, Views API).
User stories, in French, are in [`doc/user-stories.md`](doc/user-stories.md).

## License

[MIT](LICENSE) © 2026 Pierre-Henri Delaval
