# Security

Gazette runs entirely on your machine. It has no server, no account and no
telemetry.

## Data

- `~/.local/state/omarchy/gazette/config.json` and `state.json` are written
  atomically with mode `0600`; the parent directory is `0700`.
- Cached images live in `~/.cache/omarchy/gazette/images/` and are pruned at
  200 MB. They are identified by magic bytes; SVG is rejected.
- The panel renders feed content as `Text.PlainText`. Feed HTML is reduced to
  text by the engine before it reaches QML.
- Feed URLs can contain private access tokens. The panel passes the URL to the
  engine over stdin (`--add-feed -`), never in argv, so another local account
  cannot read it from `/proc/<pid>/cmdline` while the feed is added. The URL is
  stored only in `config.json` (mode `0600`).

## Network

- Only `http(s)` URLs are fetched.
- Response sizes are capped (6 MB feeds, 3 MB images, 768 KB article pages) and
  every request has connect/read timeouts.
- `og:image` lookups happen only when a feed provides no image, and only for a
  handful of the newest articles per refresh.
- Full article pages and their inline photos are fetched only after you click
  **READ FULL TEXT**, never when the panel opens.
- Turning images off stops article-page and image fetching entirely.

## External endpoints

Yahoo Finance (`query1/query2.finance.yahoo.com`), TCMB
(`tcmb.gov.tr`), open.er-api.com, plus the feeds you subscribe to. If you
subscribe to a feed, its images and (for image-less feeds) its article pages
may be requested.

## Reporting

Open a private security advisory on
<https://github.com/KaplanHalil/omarchy-gazette/security/advisories/new> or email
the address on the GitHub profile.
