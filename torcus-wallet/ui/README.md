# MPC Wallet Web UI

A clean, modern web interface for the MPC Wallet system.

## Features

- **Dashboard** - View all wallets, total balance, and system status
- **Wallet Management** - Create, view, and delete wallets (CGGMP24/Taproot)
- **Send Bitcoin** - Transfer Bitcoin using threshold signatures
- **Settings** - Configure coordinator URL and view system info

## Quick Start

### Prerequisites

1. Make sure the MPC coordinator is running on `http://localhost:3000`
2. Node.js installed (for the web server)

### Run the UI

```bash
cd ui
npm start
```

Then open your browser to: **http://localhost:8080**

### Alternative: No Installation Required

You can also open `ui/public/index.html` directly in your browser, but you'll need to configure CORS on the coordinator or use a simple HTTP server:

```bash
# Using Python
cd ui/public
python -m http.server 8080

# Using npx (no install)
cd ui/public
npx serve
```

## Configuration

The UI connects to the coordinator at `http://localhost:3000` by default. You can change this in:

1. Settings tab in the UI
2. Or edit directly in browser localStorage

## UI Structure

```
ui/
├── public/
│   ├── index.html    # Main HTML page with Tailwind CSS
│   └── app.js        # JavaScript application logic
├── server.js         # Simple Node.js HTTP server
├── package.json      # Node.js dependencies
└── README.md         # This file
```

## Screenshots

### Dashboard
- Total balance across all wallets
- Active wallet count
- MPC system status

### Wallets
- Create new wallets (CGGMP24 or Taproot)
- View wallet details (address, balance, public key)
- Delete wallets

### Send
- Select wallet
- Enter recipient address
- Specify amount in satoshis
- Set custom fee rate (optional)

### Settings
- Configure coordinator URL
- View system information (threshold, nodes, etc.)

## Development

The UI is built with:
- **HTML5** - Structure
- **Tailwind CSS** - Styling (via CDN)
- **Vanilla JavaScript** - No frameworks, just clean JS
- **Node.js HTTP server** - Simple static file server

## API Endpoints Used

The UI communicates with the coordinator using these endpoints:

- `GET /wallets` - List all wallets
- `GET /wallet/{id}` - Get wallet details
- `GET /wallet/{id}/balance` - Get wallet balance
- `POST /cggmp24/create` - Create CGGMP24 wallet
- `POST /taproot/create` - Create Taproot wallet
- `POST /cggmp24/send` - Send Bitcoin (CGGMP24)
- `POST /taproot/send` - Send Bitcoin (Taproot)
- `DELETE /wallet/{id}` - Delete wallet
- `GET /info` - Get system information

## Troubleshooting

**UI won't load wallets:**
- Check that coordinator is running: `curl http://localhost:3000/info`
- Check browser console for errors (F12)

**Can't create wallet:**
- Ensure MPC nodes are initialized (`mpc-wallet cggmp24-init`)
- Check coordinator logs for errors

**Port 8080 already in use:**
```bash
PORT=8081 npm start
```

## License

MIT
