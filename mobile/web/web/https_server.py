#!/usr/bin/env python3
import http.server
import ssl

server_address = ('0.0.0.0', 8843)
httpd = http.server.HTTPServer(server_address, http.server.SimpleHTTPRequestHandler)
ssl_context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ssl_context.load_cert_chain('cert.pem', 'key.pem')
httpd.socket = ssl_context.wrap_socket(httpd.socket, server_side=True)
print('HTTPS server on https://0.0.0.0:8843/')
httpd.serve_forever()
