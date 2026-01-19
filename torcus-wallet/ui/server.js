// Simple HTTP server for MPC Wallet UI

const http = require('http');
const fs = require('fs');
const path = require('path');

const PORT = process.env.PORT || 8080;
const PUBLIC_DIR = path.join(__dirname, 'public');

const MIME_TYPES = {
    '.html': 'text/html',
    '.js': 'application/javascript',
    '.css': 'text/css',
    '.json': 'application/json',
    '.png': 'image/png',
    '.jpg': 'image/jpeg',
    '.svg': 'image/svg+xml',
};

const server = http.createServer((req, res) => {
    console.log(`${req.method} ${req.url}`);

    // Parse URL
    let filePath = req.url === '/' ? '/index.html' : req.url;
    filePath = path.join(PUBLIC_DIR, filePath);

    // Get file extension
    const ext = path.extname(filePath);
    const contentType = MIME_TYPES[ext] || 'text/plain';

    // Read and serve file
    fs.readFile(filePath, (err, data) => {
        if (err) {
            if (err.code === 'ENOENT') {
                res.writeHead(404, { 'Content-Type': 'text/html' });
                res.end('<h1>404 Not Found</h1>');
            } else {
                res.writeHead(500);
                res.end(`Server Error: ${err.code}`);
            }
        } else {
            res.writeHead(200, { 'Content-Type': contentType });
            res.end(data);
        }
    });
});

server.listen(PORT, () => {
    console.log('='.repeat(60));
    console.log('MPC Wallet UI Server');
    console.log('='.repeat(60));
    console.log(`\n✓ Server running at: http://localhost:${PORT}`);
    console.log(`✓ Serving files from: ${PUBLIC_DIR}`);
    console.log(`\n📝 Make sure the coordinator is running on http://localhost:3000`);
    console.log(`\n🚀 Open your browser to: http://localhost:${PORT}\n`);
    console.log('='.repeat(60));
});

server.on('error', (err) => {
    if (err.code === 'EADDRINUSE') {
        console.error(`\n❌ Port ${PORT} is already in use.`);
        console.error(`   Try: PORT=8081 node server.js\n`);
    } else {
        console.error(`\n❌ Server error:`, err);
    }
    process.exit(1);
});
