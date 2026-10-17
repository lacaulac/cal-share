# cal-share

**Warning: most of the program base was vibe-coded (*i.e.*, I did not write most of this); use at your own risk.**

A production-ready Rust web API built with [Axum](https://github.com/tokio-rs/axum) and [Tokio](https://tokio.rs) for ingesting and serving iCalendar (`.ical` / `.ics`) feeds with non-expiring JWT authentication.

## Features

- **Non-Expiring JWT Authentication**: Issued JWTs identify the calendar feed by their `sub` claim and never expire.
- **Atomic File Overwrites**: Uploading a calendar for an existing identifier cleanly and atomically replaces the previous `.ics` file.
- **Zero Database Dependency**: Stores calendars directly on the local filesystem (`./data/calendars/{identifier}.ics`), making backups and volume mounting effortless.
- **Safe Path Sanitization**: Rejects path traversal attempts (`..`, `/`, invalid chars) to protect the host filesystem.
- **Sub-Millisecond Tokio Concurrency**: Powered by Tokio's multi-threaded runtime with non-blocking file I/O.

---

## API Endpoints

### 1. `POST /create_user` (Admin Token Generation)
Generates a new non-expiring JSON Web Token for the specified calendar identifier when provided with the instance's JWT secret.

**Request:**
```bash
curl -X POST http://localhost:8080/create_user \
  -H "Content-Type: application/json" \
  -d '{"secret": "your-jwt-secret", "identifier": "team-schedule"}'
```

**Response (`200 OK`):**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "identifier": "team-schedule",
  "calendar_url": "/calendar/team-schedule",
  "message": "Token generated successfully. Token does not expire."
}
```

---

### 2. `POST /upload` (Upload / Replace Calendar)
Uploads raw `.ical` / `.ics` data. The user must provide the JWT in the `Authorization: Bearer <token>` header. The calendar identifier is extracted from the token's `sub` claim. If a calendar already exists for that identifier, it is replaced atomically.

**Request:**
```bash
curl -X POST http://localhost:8080/upload \
  -H "Authorization: Bearer <YOUR_JWT_TOKEN>" \
  -H "Content-Type: text/calendar" \
  --data-binary @schedule.ics
```

**Response (`200 OK`):**
```json
{
  "status": "success",
  "identifier": "team-schedule",
  "bytes_written": 2048,
  "calendar_url": "/calendar/team-schedule"
}
```

---

### 3. `GET /calendar/:identifier` (Retrieve Calendar Feed)
Public/subscriber endpoint returning the raw `.ical` contents. Compatible with Apple Calendar, Google Calendar, and Microsoft Outlook subscription feeds.

**Request:**
```bash
curl http://localhost:8080/calendar/team-schedule
```

**Response (`200 OK`):**
```text
HTTP/1.1 200 OK
Content-Type: text/calendar; charset=utf-8
Content-Disposition: inline; filename="team-schedule.ics"
Cache-Control: no-cache, must-revalidate

BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//My Organization//On-Call Schedule//EN
...
END:VCALENDAR
```

---

## Python Integration Example

A standalone Python client is provided in `examples/upload_client.py`:

```python
from examples.upload_client import ICalClient

# Initialize client with your server URL and non-expiring JWT
client = ICalClient("http://localhost:8080", token="<YOUR_JWT_TOKEN>")

# Upload your local .ics file (replaces previous file on server)
response = client.upload_file("path/to/calendar.ics")
print(f"Uploaded {response['identifier']} successfully!")
```

---

## Running the Rust Service

### Prerequisites
- [Rust & Cargo](https://rustup.rs/) (1.80+)

### Environment Variables
| Variable | Description | Default |
|---|---|---|
| `JWT_SECRET` | Secret key used to sign and verify JWTs | *Required in production* |
| `PORT` | Listening port | `8080` |
| `HOST` | Listening host IP | `0.0.0.0` |
| `CALENDAR_STORAGE_DIR` | Directory where `.ics` files are saved | `./data/calendars` |

### Running with Cargo
```bash
# Set your production JWT secret
export JWT_SECRET="your-strong-production-secret"
export PORT="8080"

# Build and run
cargo run --release
```

### Running with Docker
```bash
# Compile before Docker image creation
cargo build --release
# Build Docker image
docker build -t cal-share .

# Run container with persistent calendar storage volume
docker run -d \
  -p 16553:8080 \
  -e JWT_SECRET="your-strong-production-secret" \
  -v $(pwd)/data/calendars:/app/data/calendars \
  --name cal-share \
  cal-share
```
