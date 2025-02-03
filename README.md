# Initial dev setup

1. Install nix
2. Install and configure direnv
3. Ask Billy to setup a sccache webdav token for you
4. Set `SCCACHE_WEBDAV_TOKEN` in `.envrc`. *DON"T PUSH THAT UP*

# Development
1. In the `./server` directory, run `cargo watch -x run`
2. in the `./client` directory, run `cargo watch -s ./serve.sh -i dist`
3. Visit [http://localhost:4000](http://localhost:4000), and refresh the page whenever changes to the server or client are made
