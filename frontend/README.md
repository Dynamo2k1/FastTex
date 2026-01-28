# FastTeX Frontend

> React-based collaborative LaTeX editor with real-time collaboration and live PDF preview.

## Table of Contents

1. [Overview](#overview)
2. [Technologies](#technologies)
3. [Prerequisites](#prerequisites)
4. [Installation](#installation)
5. [Development](#development)
6. [Architecture](#architecture)
7. [Configuration](#configuration)
8. [Features](#features)
9. [Testing](#testing)
10. [Building for Production](#building-for-production)
11. [Troubleshooting](#troubleshooting)

---

## Overview

The FastTeX frontend provides a modern, feature-rich LaTeX editing experience with:

- **Real-time collaboration** via Yjs CRDTs
- **Instant PDF preview** with SyncTeX support
- **Offline-first editing** - work without internet
- **Syntax highlighting** and intelligent code completion
- **Project file management** with drag-and-drop

---

## Technologies

| Technology | Version | Purpose |
|------------|---------|---------|
| React | 18.2+ | UI framework |
| TypeScript | 5.0+ | Type safety |
| Vite | 5.0+ | Build tool |
| CodeMirror 6 | Latest | Code editor |
| Yjs | 13.6+ | CRDT for collaboration |
| y-codemirror.next | Latest | Yjs ↔ CodeMirror binding |
| pdf.js | 4.0+ | PDF rendering |
| TanStack Query | 5.0+ | Server state management |
| Zustand | 4.4+ | Client state management |
| Tailwind CSS | 3.4+ | Styling |

---

## Prerequisites

### Required Software

- **Node.js**: 20.0.0 or higher
- **npm**: 10.0.0 or higher (or yarn 4.0+, pnpm 8.0+)
- **Git**: For version control

### Verify Installation

```bash
node --version   # Should be >= 20.0.0
npm --version    # Should be >= 10.0.0
```

### Recommended IDE Setup

**VS Code Extensions:**
- ESLint
- Prettier
- TypeScript Vue Plugin (Volar)
- Tailwind CSS IntelliSense

---

## Installation

### Step 1: Clone Repository

```bash
git clone https://github.com/fasttex/fasttex.git
cd fasttex/fasttex/frontend
```

### Step 2: Install Dependencies

```bash
# Using npm
npm install

# Using yarn
yarn install

# Using pnpm
pnpm install
```

### Step 3: Configure Environment

```bash
# Copy example environment file
cp .env.example .env.local

# Edit with your settings
nano .env.local
```

**Environment Variables (minimum for dev):**

```env
# API / WebSocket endpoints (match backend gateway)
VITE_API_URL=http://localhost:8080
VITE_WS_URL=ws://localhost:8080

# Optional: change Vite dev server port (defaults to 5173)
# VITE_DEV_PORT=5173
```

> The frontend proxies `/api`, `/sync`, and `/compile` to the gateway at `VITE_API_URL` / `VITE_WS_URL`. Ensure the gateway is running (see root README) before starting dev server to avoid `ECONNREFUSED 127.0.0.1:8080` errors in the Vite console.

### Step 4: Start Development Server

```bash
npm run dev
```

Vite will start on http://localhost:5173 by default (or `VITE_DEV_PORT` if set).

---

## Development

### Available Scripts

| Script | Description |
|--------|-------------|
| `npm run dev` | Start development server with hot reload |
| `npm run build` | Build for production |
| `npm run preview` | Preview production build locally |
| `npm run lint` | Run ESLint |
| `npm run lint:fix` | Fix auto-fixable lint issues |
| `npm run format` | Format code with Prettier |
| `npm run typecheck` | Run TypeScript type checking |
| `npm run test` | Run unit tests |
| `npm run test:watch` | Run tests in watch mode |
| `npm run test:coverage` | Run tests with coverage report |
| `npm run e2e` | Run end-to-end tests |

### Development Workflow

1. **Start the backend services** (see main README)
2. **Start the frontend** with `npm run dev`
3. **Make changes** - hot reload will update automatically
4. **Run linting** before committing: `npm run lint`
5. **Run tests** to verify changes: `npm run test`

### Debugging

**Browser DevTools:**
- React DevTools extension for component inspection
- Network tab for API/WebSocket traffic
- Console for debug logs

**VS Code:**
```json
// .vscode/launch.json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "chrome",
      "request": "launch",
      "name": "Debug FastTeX",
      "url": "http://localhost:3000",
      "webRoot": "${workspaceFolder}/src"
    }
  ]
}
```

---

## Architecture

### Directory Structure

```
src/
├── components/           # React components
│   ├── Editor/          # CodeMirror editor with Yjs
│   │   ├── Editor.tsx
│   │   ├── extensions/  # CodeMirror extensions
│   │   └── themes/      # Editor themes
│   ├── Preview/         # PDF preview panel
│   │   ├── Preview.tsx
│   │   └── SyncTeX.ts   # Click-to-source logic
│   ├── FileTree/        # Project file browser
│   │   ├── FileTree.tsx
│   │   └── FileNode.tsx
│   ├── Toolbar/         # Compile controls, settings
│   ├── Presence/        # Collaborator avatars/cursors
│   └── common/          # Shared components
├── hooks/               # Custom React hooks
│   ├── useYjs.ts        # Yjs document management
│   ├── useCompile.ts    # Compilation state/triggers
│   ├── usePresence.ts   # Presence awareness
│   ├── useProject.ts    # Project data fetching
│   └── useAuth.ts       # Authentication state
├── services/            # API and WebSocket clients
│   ├── api.ts           # REST API client
│   ├── websocket.ts     # WebSocket connection manager
│   └── storage.ts       # Local storage helpers
├── stores/              # Zustand state stores
│   ├── editorStore.ts   # Editor state
│   ├── projectStore.ts  # Project state
│   └── uiStore.ts       # UI state (panels, modals)
├── utils/               # Utility functions
│   ├── latex.ts         # LaTeX helpers
│   └── formatting.ts    # Text formatting
├── types/               # TypeScript type definitions
├── App.tsx              # Root component
├── main.tsx             # Entry point
└── index.css            # Global styles
```

### State Management

```
┌─────────────────────────────────────────────────────────────┐
│                      React Components                        │
└─────────────────────────┬───────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
    ┌──────────┐    ┌──────────┐    ┌──────────┐
    │ Zustand  │    │  Yjs     │    │ TanStack │
    │ (UI/App) │    │ (Editor) │    │  Query   │
    │  State   │    │  State   │    │ (Server) │
    └──────────┘    └──────────┘    └──────────┘
          │               │               │
          │               │               ▼
          │               │         ┌──────────┐
          │               │         │   API    │
          │               └────────►│ Service  │
          │                         └──────────┘
          │                               │
          └───────────────────────────────┘
```

---

## Configuration

### Theme Configuration

```typescript
// src/config/theme.ts
export const editorThemes = {
  light: {
    background: '#ffffff',
    foreground: '#24292e',
    selection: '#c8c8fa',
    cursor: '#24292e',
    // ...
  },
  dark: {
    background: '#1e1e1e',
    foreground: '#d4d4d4',
    selection: '#264f78',
    cursor: '#aeafad',
    // ...
  },
};
```

### Keybindings

```typescript
// src/config/keybindings.ts
export const keybindings = {
  save: 'Mod-s',
  compile: 'Mod-Enter',
  find: 'Mod-f',
  replace: 'Mod-h',
  togglePreview: 'Mod-Shift-p',
  // ...
};
```

---

## Features

### Real-time Collaboration

The editor uses Yjs CRDTs for conflict-free collaborative editing:

1. **Local edits** are applied immediately for instant feedback
2. **Changes are broadcast** via WebSocket to other users
3. **Remote changes** are merged automatically without conflicts
4. **Offline edits** sync seamlessly when reconnecting

```typescript
// Example: Connecting to collaboration
const { doc, provider, connected } = useYjs(projectId);

// Access shared text
const sharedText = doc.getText('main.tex');

// Listen for changes
sharedText.observe((event) => {
  console.log('Document changed:', event);
});
```

### PDF Preview

PDF rendering uses pdf.js with features:

- **Live preview** during compilation
- **SyncTeX support** for bidirectional navigation
  - Click in PDF → Jump to source
  - Click in source → Highlight in PDF
- **Zoom controls** and page navigation
- **Thumbnail sidebar** for quick navigation

### Offline Support

When offline, FastTeX:

1. **Saves edits locally** using IndexedDB
2. **Queues compilation requests** for later
3. **Syncs automatically** when online
4. **Shows offline indicator** in UI

```typescript
// Check online status
const { isOnline, pendingChanges } = useOfflineStatus();

if (!isOnline) {
  showNotification('Working offline. Changes will sync when online.');
}
```

---

## Testing

### Unit Tests

```bash
# Run all tests
npm run test

# Run in watch mode
npm run test:watch

# With coverage
npm run test:coverage
```

**Example Test:**
```typescript
// src/components/Editor/Editor.test.tsx
import { render, screen } from '@testing-library/react';
import { Editor } from './Editor';

describe('Editor', () => {
  it('renders with initial content', () => {
    render(
      <Editor
        projectId="test-123"
        initialContent="\\documentclass{article}"
      />
    );
    
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });
});
```

### E2E Tests

```bash
# Run Playwright tests
npm run e2e

# With UI
npm run e2e:ui
```

---

## Building for Production

### Build

```bash
npm run build
```

Output is in `dist/` directory.

### Preview Build

```bash
npm run preview
```

### Deployment

The build output is static and can be served from any static host:

- **Nginx**: Copy `dist/` to web root
- **Vercel**: Deploy directly from Git
- **Netlify**: Deploy directly from Git
- **S3 + CloudFront**: Upload `dist/` to S3

**Nginx Configuration:**
```nginx
server {
    root /var/www/fasttex;
    index index.html;
    
    location / {
        try_files $uri $uri/ /index.html;
    }
    
    location ~* \.(js|css|png|jpg|jpeg|gif|ico|svg|woff|woff2)$ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }
}
```

---

## Troubleshooting

### Common Issues

#### 1. "Module not found" errors

```bash
# Clear node_modules and reinstall
rm -rf node_modules package-lock.json
npm install
```

#### 2. Hot reload not working

```bash
# Check Vite is watching files
# May need to increase file watcher limit on Linux:
echo fs.inotify.max_user_watches=524288 | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

#### 3. WebSocket connection failed

- Verify backend gateway is running on correct port
- Check CORS configuration
- Verify `VITE_WS_URL` in environment

#### 4. PDF not rendering

- Check browser console for pdf.js errors
- Verify PDF worker is loaded correctly
- Try clearing browser cache

#### 5. TypeScript errors

```bash
# Run type checking
npm run typecheck

# Clear TypeScript cache
rm -rf node_modules/.cache
```

### Getting Help

- **Documentation**: [docs.fasttex.io](https://docs.fasttex.io)
- **Discord**: [discord.gg/fasttex](https://discord.gg/fasttex)
- **GitHub Issues**: [github.com/fasttex/fasttex/issues](https://github.com/fasttex/fasttex/issues)
