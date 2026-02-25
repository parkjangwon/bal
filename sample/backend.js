const http = require('http');
const url = require('url');

// Get port from command line argument or use default
const port = process.argv[2] || 9000;
const serverId = `backend-${port}`;

const server = http.createServer((req, res) => {
  const timestamp = new Date().toISOString();
  const parsedUrl = url.parse(req.url, true);
  
  // Log incoming request
  console.log(`[${timestamp}] ${serverId} received request:`);
  console.log(`  Method: ${req.method}`);
  console.log(`  Path: ${parsedUrl.pathname}`);
  console.log(`  Query: ${JSON.stringify(parsedUrl.query)}`);
  console.log(`  Headers: ${JSON.stringify(req.headers, null, 2)}`);
  console.log('---');
  
  // Simulate some processing time (optional)
  // setTimeout(() => {
  
  res.writeHead(200, { 
    'Content-Type': 'application/json',
    'X-Backend-Server': serverId
  });
  
  const response = {
    status: 'success',
    message: `Hello from ${serverId}!`,
    timestamp: timestamp,
    request: {
      method: req.method,
      path: parsedUrl.pathname,
      query: parsedUrl.query
    },
    server: {
      id: serverId,
      port: port
    }
  };
  
  res.end(JSON.stringify(response, null, 2));
  
  // }, 10); // 10ms delay
});

server.listen(port, '127.0.0.1', () => {
  console.log(`
========================================
  ${serverId} is running!
  Port: ${port}
  URL: http://127.0.0.1:${port}
========================================
  `);
  console.log('Waiting for incoming requests...\n');
});

// Handle graceful shutdown
process.on('SIGTERM', () => {
  console.log(`\n[${serverId}] SIGTERM received. Shutting down gracefully...`);
  server.close(() => {
    console.log(`[${serverId}] Server closed.`);
    process.exit(0);
  });
});

process.on('SIGINT', () => {
  console.log(`\n[${serverId}] SIGINT received. Shutting down gracefully...`);
  server.close(() => {
    console.log(`[${serverId}] Server closed.`);
    process.exit(0);
  });
});
