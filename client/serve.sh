#!/bin/sh

./bindgen.sh shooter3

# Start basic HTTP server
echo "Starting basic HTTP server..."
basic-http-server ./dist
