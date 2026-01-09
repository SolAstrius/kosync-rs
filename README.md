# kosync-rs

Extended KOReader sync server and plugin with annotation support.

## Components

- **server/** - Rust sync server (replaces Lua/OpenResty kosync)
- **plugin/** - KOReader plugin with extended sync capabilities

## Features

### Original KOSync API (compatible)
- User registration/login
- Reading progress sync (position, percentage, device)

### Extended API
- Annotation sync (bookmarks, highlights, notes)
- Timestamp-based merge with conflict resolution
- Deletion tracking

## Server

### Build & Run

```bash
cd server
cargo build --release
./target/release/kosync-server
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `KOSYNC_PORT` | `7200` | Server port |
| `KOSYNC_DB_PATH` | `kosync.db` | Database file path |
| `RUST_LOG` | `info` | Log level |

### API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/users/create` | Register user |
| GET | `/users/auth` | Verify credentials |
| PUT | `/syncs/progress` | Update reading progress |
| GET | `/syncs/progress/:document` | Get reading progress |
| GET | `/syncs/annotations/:document` | Get annotations |
| PUT | `/syncs/annotations/:document` | Update annotations |
| GET | `/healthcheck` | Health check |

## Plugin

### Installation

Copy `plugin/kosync-ext.koplugin/` to your KOReader plugins directory:

```bash
cp -r plugin/kosync-ext.koplugin ~/.config/koreader/plugins/
# or for device
cp -r plugin/kosync-ext.koplugin /mnt/koreader/plugins/
```

### Usage

1. Open any book in KOReader
2. Go to menu → Extended Sync
3. Set your custom server URL
4. Register or login
5. Enable "Auto sync on open/close" and "Sync annotations"

## License

AGPL-3.0 (matching original kosync)
