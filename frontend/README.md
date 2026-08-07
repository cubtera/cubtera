# Cubtera Frontend

React TypeScript frontend for Cubtera Infrastructure Manager.

## Tech Stack

- **React 18** + **TypeScript**
- **Vite** for build tooling
- **shadcn/ui** + **TailwindCSS** for UI components
- **Tanstack Query** for server state
- **React Router v6** for routing
- **Zustand** for client state

## Development

```bash
# Install dependencies
npm install

# Start development server
npm run dev

# Type checking
npm run type-check

# Build for production
npm run build
```

## Project Structure

```
src/
├── components/          # Reusable UI components
│   ├── ui/             # shadcn/ui base components
│   ├── layout/         # Layout components
│   └── domain/         # Domain-specific components
├── pages/              # Route pages
├── hooks/              # Custom React hooks
├── api/                # API client and types
├── store/              # Zustand stores
├── types/              # TypeScript type definitions
└── utils/              # Helper utilities
```

## API Integration

The frontend connects to the Rust backend API running on port 8000. The Vite dev server proxies `/api/*` requests to `localhost:8000`.

## Environment Variables

- `VITE_API_URL` - Backend API URL (default: http://localhost:8000)

## Features

- 📊 Infrastructure dashboard
- 🗂️ Dimension management
- 📦 Unit browser
- 📜 Deployment logs
- ⚙️ Configuration management
- 🌙 Dark/light theme support
- 📱 Responsive design 