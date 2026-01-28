# FastTeX API Reference

> Complete REST and WebSocket API documentation for FastTeX.

## Table of Contents

1. [Authentication](#authentication)
2. [REST API](#rest-api)
3. [WebSocket API](#websocket-api)
4. [Error Codes](#error-codes)
5. [Rate Limits](#rate-limits)

---

## Authentication

FastTeX uses JWT (JSON Web Tokens) for authentication.

### Obtaining a Token

```http
POST /api/auth/login
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "your-password"
}
```

**Response:**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIs...",
  "user": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "email": "user@example.com",
    "name": "John Doe",
    "created_at": "2024-01-15T10:30:00Z"
  },
  "expires_at": "2024-01-16T10:30:00Z"
}
```

### Using the Token

Include the token in the `Authorization` header:

```http
Authorization: Bearer eyJhbGciOiJIUzI1NiIs...
```

For WebSocket connections, pass the token as a query parameter:

```
wss://api.fasttex.io/ws/sync/PROJECT_ID?token=eyJhbGciOiJIUzI1NiIs...
```

---

## REST API

### Base URL

```
Production: https://api.fasttex.io
Development: http://localhost:8080
```

### Authentication Endpoints

#### Sign Up

```http
POST /api/auth/signup
Content-Type: application/json

{
  "email": "newuser@example.com",
  "password": "secure-password-123",
  "name": "Jane Doe"
}
```

**Response (201 Created):**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440001",
  "email": "newuser@example.com",
  "name": "Jane Doe",
  "created_at": "2024-01-15T11:00:00Z"
}
```

#### Login

```http
POST /api/auth/login
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "your-password"
}
```

#### Logout

```http
POST /api/auth/logout
Authorization: Bearer <token>
```

#### Refresh Token

```http
POST /api/auth/refresh
Authorization: Bearer <token>
```

**Response:**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIs...",
  "expires_at": "2024-01-16T10:30:00Z"
}
```

---

### Project Endpoints

#### List Projects

```http
GET /api/projects
Authorization: Bearer <token>
```

**Query Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `page` | int | Page number (default: 1) |
| `per_page` | int | Items per page (default: 20, max: 100) |
| `sort` | string | Sort field: `created_at`, `updated_at`, `name` |
| `order` | string | Sort order: `asc`, `desc` |

**Response:**
```json
{
  "projects": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440002",
      "name": "My Thesis",
      "description": "PhD thesis on quantum computing",
      "created_at": "2024-01-10T08:00:00Z",
      "updated_at": "2024-01-15T14:30:00Z",
      "owner_id": "550e8400-e29b-41d4-a716-446655440000",
      "collaborators_count": 3,
      "files_count": 15
    }
  ],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 5,
    "total_pages": 1
  }
}
```

#### Create Project

```http
POST /api/projects
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "New Paper",
  "description": "Research paper on machine learning",
  "template": "article",
  "git_url": "https://github.com/user/paper.git"
}
```

**Available Templates:**
- `blank` - Empty project
- `article` - Standard article template
- `book` - Book/thesis template
- `beamer` - Presentation template
- `letter` - Letter template

**Response (201 Created):**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440003",
  "name": "New Paper",
  "description": "Research paper on machine learning",
  "created_at": "2024-01-15T15:00:00Z",
  "updated_at": "2024-01-15T15:00:00Z",
  "owner_id": "550e8400-e29b-41d4-a716-446655440000",
  "main_file": "main.tex"
}
```

#### Get Project

```http
GET /api/projects/:id
Authorization: Bearer <token>
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440002",
  "name": "My Thesis",
  "description": "PhD thesis on quantum computing",
  "created_at": "2024-01-10T08:00:00Z",
  "updated_at": "2024-01-15T14:30:00Z",
  "owner_id": "550e8400-e29b-41d4-a716-446655440000",
  "main_file": "main.tex",
  "compiler": "tectonic",
  "collaborators": [
    {
      "user_id": "550e8400-e29b-41d4-a716-446655440004",
      "email": "collaborator@example.com",
      "name": "Bob Smith",
      "role": "editor"
    }
  ],
  "settings": {
    "auto_compile": true,
    "compile_timeout": 300,
    "spell_check": true,
    "spell_check_language": "en-US"
  }
}
```

#### Update Project

```http
PUT /api/projects/:id
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "Updated Name",
  "description": "New description",
  "main_file": "thesis.tex",
  "settings": {
    "auto_compile": false,
    "compile_timeout": 600
  }
}
```

#### Delete Project

```http
DELETE /api/projects/:id
Authorization: Bearer <token>
```

**Response (204 No Content)**

---

### File Endpoints

#### List Files

```http
GET /api/projects/:project_id/files
Authorization: Bearer <token>
```

**Response:**
```json
{
  "files": [
    {
      "path": "main.tex",
      "type": "file",
      "size": 2048,
      "modified_at": "2024-01-15T14:30:00Z"
    },
    {
      "path": "chapters",
      "type": "directory",
      "children": [
        {
          "path": "chapters/introduction.tex",
          "type": "file",
          "size": 5120,
          "modified_at": "2024-01-14T10:00:00Z"
        },
        {
          "path": "chapters/methodology.tex",
          "type": "file",
          "size": 8192,
          "modified_at": "2024-01-15T09:00:00Z"
        }
      ]
    },
    {
      "path": "figures",
      "type": "directory",
      "children": []
    }
  ]
}
```

#### Get File Content

```http
GET /api/projects/:project_id/files/:path
Authorization: Bearer <token>
```

**Response:**
```json
{
  "path": "main.tex",
  "content": "\\documentclass{article}\n\\begin{document}\n...",
  "encoding": "utf-8",
  "size": 2048,
  "modified_at": "2024-01-15T14:30:00Z",
  "content_hash": "sha256:abc123..."
}
```

#### Create/Update File

```http
PUT /api/projects/:project_id/files/:path
Authorization: Bearer <token>
Content-Type: application/json

{
  "content": "\\documentclass{article}\n...",
  "encoding": "utf-8"
}
```

#### Upload Binary File

```http
POST /api/projects/:project_id/files/:path
Authorization: Bearer <token>
Content-Type: multipart/form-data

file=@image.png
```

#### Delete File

```http
DELETE /api/projects/:project_id/files/:path
Authorization: Bearer <token>
```

---

### Compilation Endpoints

#### Trigger Compilation

```http
POST /api/projects/:project_id/compile
Authorization: Bearer <token>
Content-Type: application/json

{
  "target": "main.tex",
  "mode": "full",
  "draft": false
}
```

**Compilation Modes:**
- `full` - Complete recompilation
- `incremental` - Only changed files
- `quick` - Single pass, no cross-references

**Response (202 Accepted):**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440010",
  "status": "queued",
  "created_at": "2024-01-15T15:30:00Z",
  "estimated_duration": 15
}
```

#### Get Compilation Status

```http
GET /api/projects/:project_id/compile/:job_id
Authorization: Bearer <token>
```

**Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440010",
  "status": "compiling",
  "progress": 0.65,
  "current_task": "Compiling chapter2.tex",
  "started_at": "2024-01-15T15:30:05Z",
  "tasks": [
    { "name": "preamble", "status": "completed", "duration": 2.5 },
    { "name": "chapter1", "status": "completed", "duration": 3.2 },
    { "name": "chapter2", "status": "running" },
    { "name": "chapter3", "status": "pending" },
    { "name": "merge", "status": "pending" }
  ]
}
```

**Status Values:**
- `queued` - Waiting to start
- `compiling` - In progress
- `merging` - Merging PDF fragments
- `converging` - Running additional passes for cross-references
- `completed` - Finished successfully
- `failed` - Compilation failed

#### Get Compilation Output

```http
GET /api/projects/:project_id/compile/:job_id/output
Authorization: Bearer <token>
```

**Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440010",
  "status": "completed",
  "pdf_url": "/api/projects/.../pdf/latest",
  "log": "This is pdfTeX...\n[1] [2] [3]...",
  "diagnostics": [
    {
      "severity": "warning",
      "message": "Underfull \\hbox (badness 10000) in paragraph",
      "file": "chapter1.tex",
      "line": 42
    }
  ],
  "stats": {
    "pages": 25,
    "duration_seconds": 12.5,
    "cache_hits": 3,
    "parallel_jobs": 4
  }
}
```

#### Get PDF

```http
GET /api/projects/:project_id/pdf/latest
Authorization: Bearer <token>
Accept: application/pdf
```

**Response:** Binary PDF file

#### Cancel Compilation

```http
DELETE /api/projects/:project_id/compile/:job_id
Authorization: Bearer <token>
```

---

### Collaborator Endpoints

#### List Collaborators

```http
GET /api/projects/:project_id/collaborators
Authorization: Bearer <token>
```

**Response:**
```json
{
  "collaborators": [
    {
      "user_id": "550e8400-e29b-41d4-a716-446655440004",
      "email": "collaborator@example.com",
      "name": "Bob Smith",
      "role": "editor",
      "added_at": "2024-01-12T10:00:00Z"
    }
  ]
}
```

#### Add Collaborator

```http
POST /api/projects/:project_id/collaborators
Authorization: Bearer <token>
Content-Type: application/json

{
  "email": "newcollaborator@example.com",
  "role": "editor"
}
```

**Roles:**
- `viewer` - Read-only access
- `editor` - Can edit files
- `admin` - Can manage project settings

#### Update Collaborator Role

```http
PUT /api/projects/:project_id/collaborators/:user_id
Authorization: Bearer <token>
Content-Type: application/json

{
  "role": "admin"
}
```

#### Remove Collaborator

```http
DELETE /api/projects/:project_id/collaborators/:user_id
Authorization: Bearer <token>
```

---

### Git Endpoints

#### Get Git Status

```http
GET /api/projects/:project_id/git/status
Authorization: Bearer <token>
```

**Response:**
```json
{
  "branch": "main",
  "remote": "https://github.com/user/paper.git",
  "ahead": 2,
  "behind": 0,
  "modified": ["chapter1.tex"],
  "untracked": ["new-figure.png"]
}
```

#### Sync with Git

```http
POST /api/projects/:project_id/git/sync
Authorization: Bearer <token>
Content-Type: application/json

{
  "direction": "pull",
  "message": "Updated from FastTeX"
}
```

**Direction:**
- `pull` - Pull changes from remote
- `push` - Push changes to remote
- `sync` - Pull then push

#### Get Git History

```http
GET /api/projects/:project_id/git/history
Authorization: Bearer <token>
```

**Response:**
```json
{
  "commits": [
    {
      "sha": "abc123def456...",
      "message": "Updated chapter 2",
      "author": "John Doe",
      "date": "2024-01-15T14:00:00Z"
    }
  ]
}
```

---

## WebSocket API

### Connection

```javascript
const ws = new WebSocket('wss://api.fasttex.io/ws/sync/PROJECT_ID?token=JWT_TOKEN');
```

### Message Types

#### Client → Server

**Yjs Sync Update:**
```json
{
  "type": "sync",
  "payload": "<base64-encoded Yjs update>"
}
```

**Presence Update:**
```json
{
  "type": "presence",
  "cursor": {
    "file": "main.tex",
    "line": 42,
    "column": 10
  },
  "selection": {
    "start": { "line": 42, "column": 5 },
    "end": { "line": 42, "column": 15 }
  }
}
```

**Trigger Compile:**
```json
{
  "type": "compile",
  "mode": "incremental",
  "target": "main.tex"
}
```

**Ping (Keepalive):**
```json
{
  "type": "ping"
}
```

#### Server → Client

**Yjs Sync Update:**
```json
{
  "type": "sync",
  "payload": "<base64-encoded Yjs update>"
}
```

**Presence Update:**
```json
{
  "type": "presence",
  "user_id": "550e8400-e29b-41d4-a716-446655440004",
  "name": "Bob Smith",
  "color": "#ff6b6b",
  "cursor": {
    "file": "chapter1.tex",
    "line": 100,
    "column": 25
  }
}
```

**User Joined:**
```json
{
  "type": "user_joined",
  "user_id": "550e8400-e29b-41d4-a716-446655440004",
  "name": "Bob Smith",
  "color": "#ff6b6b"
}
```

**User Left:**
```json
{
  "type": "user_left",
  "user_id": "550e8400-e29b-41d4-a716-446655440004"
}
```

**Compile Status:**
```json
{
  "type": "compile_status",
  "job_id": "550e8400-e29b-41d4-a716-446655440010",
  "status": "compiling",
  "progress": 0.75,
  "current_task": "Compiling chapter3.tex"
}
```

**Compile Complete:**
```json
{
  "type": "compile_complete",
  "job_id": "550e8400-e29b-41d4-a716-446655440010",
  "success": true,
  "pdf_url": "/api/projects/.../pdf/latest",
  "diagnostics": [...]
}
```

**Compile Error:**
```json
{
  "type": "compile_error",
  "job_id": "550e8400-e29b-41d4-a716-446655440010",
  "error": "LaTeX Error: File `missing.sty' not found.",
  "file": "main.tex",
  "line": 5
}
```

**Pong:**
```json
{
  "type": "pong"
}
```

---

## Error Codes

### HTTP Status Codes

| Code | Description |
|------|-------------|
| 200 | Success |
| 201 | Created |
| 202 | Accepted (async operation started) |
| 204 | No Content (successful deletion) |
| 400 | Bad Request - Invalid input |
| 401 | Unauthorized - Invalid/missing token |
| 403 | Forbidden - Insufficient permissions |
| 404 | Not Found |
| 409 | Conflict - Resource already exists |
| 422 | Unprocessable Entity - Validation failed |
| 429 | Too Many Requests - Rate limited |
| 500 | Internal Server Error |
| 503 | Service Unavailable |

### Error Response Format

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid email format",
    "details": {
      "field": "email",
      "value": "invalid-email"
    }
  },
  "request_id": "req-abc123"
}
```

### Error Codes

| Code | Description |
|------|-------------|
| `AUTH_INVALID_CREDENTIALS` | Wrong email or password |
| `AUTH_TOKEN_EXPIRED` | JWT has expired |
| `AUTH_TOKEN_INVALID` | JWT is malformed |
| `PROJECT_NOT_FOUND` | Project doesn't exist |
| `FILE_NOT_FOUND` | File doesn't exist |
| `PERMISSION_DENIED` | User lacks required permission |
| `VALIDATION_ERROR` | Input validation failed |
| `COMPILE_TIMEOUT` | Compilation exceeded time limit |
| `COMPILE_FAILED` | LaTeX compilation error |
| `RATE_LIMIT_EXCEEDED` | Too many requests |
| `STORAGE_ERROR` | File storage error |

---

## Rate Limits

| Endpoint | Limit | Window |
|----------|-------|--------|
| Authentication | 10 requests | 1 minute |
| Project List | 60 requests | 1 minute |
| File Operations | 120 requests | 1 minute |
| Compile | 20 requests | 1 minute |
| WebSocket messages | 100 messages | 1 second |

**Rate Limit Headers:**
```http
X-RateLimit-Limit: 60
X-RateLimit-Remaining: 45
X-RateLimit-Reset: 1705330800
```

When rate limited, you'll receive:
```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Too many requests. Please wait 30 seconds.",
    "retry_after": 30
  }
}
```

---

## SDK Examples

### JavaScript/TypeScript

```typescript
import { FastTeXClient } from '@fasttex/sdk';

const client = new FastTeXClient({
  baseUrl: 'https://api.fasttex.io',
  token: 'your-jwt-token'
});

// List projects
const projects = await client.projects.list();

// Create project
const project = await client.projects.create({
  name: 'My Paper',
  template: 'article'
});

// Connect to real-time sync
const sync = await client.sync.connect(project.id);

sync.on('presence', (user) => {
  console.log(`${user.name} is editing ${user.cursor.file}`);
});

sync.on('compile_complete', (result) => {
  console.log(`Compilation finished: ${result.pdf_url}`);
});
```

### Python

```python
from fasttex import FastTeXClient

client = FastTeXClient(
    base_url="https://api.fasttex.io",
    token="your-jwt-token"
)

# List projects
projects = client.projects.list()

# Create project
project = client.projects.create(
    name="My Paper",
    template="article"
)

# Trigger compilation
job = client.compile.trigger(project.id, mode="incremental")

# Wait for completion
result = client.compile.wait(project.id, job.job_id)
print(f"PDF available at: {result.pdf_url}")
```

### cURL

```bash
# Login
curl -X POST https://api.fasttex.io/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"user@example.com","password":"password"}'

# Create project
curl -X POST https://api.fasttex.io/api/projects \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"My Paper","template":"article"}'

# Trigger compile
curl -X POST https://api.fasttex.io/api/projects/$PROJECT_ID/compile \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"mode":"full"}'

# Download PDF
curl -O https://api.fasttex.io/api/projects/$PROJECT_ID/pdf/latest \
  -H "Authorization: Bearer $TOKEN"
```
