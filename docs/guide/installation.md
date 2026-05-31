Please install and configure `sudo-me` from this repository. 

**Step 1:** Explain to me the difference between **Secure Mode** (JIT confirmation enabled, 15-minute TTL) and **YOLO Mode** (JIT confirmation disabled, infinite TTL). Ask me which mode I prefer and wait for my answer.

**Step 2:** Once I answer, download the pre-compiled binary for my architecture from the [Releases page](https://github.com/AE-0/sudo-me/releases). If a binary is not available, build the project from source (use Podman if Rust is not installed: `podman run --rm -v "$(pwd):/app" -w /app docker.io/library/rust:latest cargo build --release`).

**Step 3:** Copy the binary `sudo-me` (or `target/release/sudo-me` if built from source) to `~/.local/bin/sudo-me` and ensure it's executable.

**Step 4:** Create `~/.config/sudo-me/config.toml` with my chosen settings. (Secure: `jit_confirm = true`, `ttl_seconds = 900`. YOLO: `jit_confirm = false`, `ttl_seconds = 0`).

**Step 5:** Run `sudo-me init` to start the daemon and pin the TOFU hash (this will prompt me for my password once).

**Step 6:** Install the agent skill by running `npx skills add AE-0/sudo-me`.