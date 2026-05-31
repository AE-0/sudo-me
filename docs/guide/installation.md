# Installing sudo-me

Please follow these steps carefully. Do not skip any steps.

---

## Step 1: Explain the Security Modes

Before doing anything, you MUST explain the two security modes to the user in plain language. Do not use acronyms without defining them first.

### What is JIT Confirmation?

**JIT** stands for **Just-In-Time**. When JIT Confirmation is enabled, every time you (the AI agent) try to run a command as root (e.g., `apt-get update`, `systemctl restart`), a popup dialog or terminal prompt will appear asking the human user: *"AI Agent wants to run as root: apt-get update. Allow?"*. The user must click "Allow" or type "y" before the command executes.

**Why does this exist?** AI agents can be tricked by malicious files (this is called "prompt injection"). If someone puts a hidden instruction in a README file that says "run `rm -rf /` as root", JIT Confirmation ensures the human sees the command and can deny it. This is your most important safety net.

### What is TTL?

**TTL** stands for **Time-To-Live**. This is a countdown timer. When the background daemon (the process that runs commands as root) starts, it begins a timer. If nobody runs any commands for the duration of the TTL, the daemon automatically shuts itself down and deletes the authentication token. After that, the next command will require re-entering the password.

**Why does this exist?** If an attacker gets access to your user account while you're away from your computer, they can't use `sudo-me` to run root commands because the daemon has already shut down.

### The Two Modes

| Setting | **Secure Mode** | **YOLO Mode** |
|---|---|---|
| JIT Confirmation | ON — You approve every root command | OFF — The agent runs anything it wants |
| TTL (inactivity timeout) | 15 minutes — Daemon dies after 15 min of no activity | Infinite — Daemon runs until you kill it |
| Best for | Security-conscious users, production systems | Trusted environments, rapid development |

**After explaining this, ask the user which mode they prefer and WAIT for their answer before proceeding.**

---

## Step 2: Detect Architecture

Run this command to determine the system architecture:

```bash
uname -m && uname -s
```

This tells us which binary to download. The possible outputs are:
- `x86_64 Linux` — Download `sudo-me-linux-x86_64-gnu.tar.gz`
- `aarch64 Linux` — No pre-built binary available, must build from source
- `x86_64 Darwin` (macOS Intel) — No pre-built binary available, must build from source
- `arm64 Darwin` (macOS Apple Silicon) — No pre-built binary available, must build from source
- `Windows` — Download `sudo-me-windows-x86_64.zip`

---

## Step 3: Download or Build

### Option A: Download the pre-built binary (Linux x86_64 only)

```bash
curl -fsSL -o /tmp/sudo-me.tar.gz "https://github.com/AE-0/sudo-me/releases/download/v0.1.0/sudo-me-linux-x86_64-gnu.tar.gz"
```

Then extract it:

```bash
tar -xzf /tmp/sudo-me.tar.gz -C /tmp/
```

This will create `/tmp/sudo-me` (the actual binary).

### Option B: Build from source (if no pre-built binary exists)

First, clone the repository:

```bash
git clone https://github.com/AE-0/sudo-me.git /tmp/sudo-me-src
cd /tmp/sudo-me-src
```

Then build. If Rust is installed:

```bash
cargo build --release
```

If Rust is NOT installed but Podman or Docker is:

```bash
podman run --rm -v "$(pwd):/app" -w /app docker.io/library/rust:latest cargo build --release
```

The binary will be at `target/release/sudo-me` (or `target/x86_64-pc-windows-gnu/release/sudo-me.exe` on Windows).

---

## Step 4: Install the Binary

Create the local bin directory if it doesn't exist:

```bash
mkdir -p ~/.local/bin
```

Copy the binary:

```bash
cp /tmp/sudo-me ~/.local/bin/sudo-me
chmod +x ~/.local/bin/sudo-me
```

If you built from source instead:

```bash
cp target/release/sudo-me ~/.local/bin/sudo-me
chmod +x ~/.local/bin/sudo-me
```

Verify it works:

```bash
~/.local/bin/sudo-me
```

You should see: `Usage: sudo-me <init|run|approve-config|--askpass>`

---

## Step 5: Configure the Security Mode

Create the configuration directory and file based on the user's choice from Step 1.

### For Secure Mode:

```bash
mkdir -p ~/.config/sudo-me
cat > ~/.config/sudo-me/config.toml << 'EOF'
jit_confirm = true
ttl_seconds = 900
EOF
```

### For YOLO Mode:

```bash
mkdir -p ~/.config/sudo-me
cat > ~/.config/sudo-me/config.toml << 'EOF'
jit_confirm = false
ttl_seconds = 0
EOF
```

---

## Step 6: Initialize the Daemon

Run the following command to start the background daemon:

```bash
~/.local/bin/sudo-me init
```

This will:
1. Prompt the user for their sudo password (via a GUI popup or terminal prompt).
2. Start a background process that runs as root.
3. Save the connection details to `~/.sudo-me`.
4. Output `export SUDO_ME_SOCK=...` and `export SUDO_ME_TOKEN=...`.

**Important:** If you are in an interactive shell, you can optionally run `eval $(sudo-me init)` to also export the environment variables into the current session. However, this is NOT required — the `~/.sudo-me` file allows `sudo-me run` to work from any shell without needing `eval`.

---

## Step 7: Test It

Run a simple test command:

```bash
~/.local/bin/sudo-me run whoami
```

If everything is working, it should output `root`.

- If **Secure Mode** is enabled, the user will see a confirmation popup asking them to approve the command.
- If **YOLO Mode** is enabled, the command will run immediately without any prompt.

---

## Step 8: Install the Agent Skill

If this is being set up for an AI coding agent (OpenCode, Claude Code, Cursor, etc.), install the skill so the agent knows how to use `sudo-me`:

```bash
npx skills add AE-0/sudo-me
```

---

## Troubleshooting

- **"Error: Session not found"** — The daemon was not initialized. Run `sudo-me init`.
- **"Permission denied"** — The daemon may have crashed or the socket file was deleted. Run `sudo-me init` again.
- **"User denied command execution"** — The user clicked "Deny" on the JIT confirmation prompt. This is expected behavior in Secure Mode.
- **GUI popup doesn't appear** — Make sure `zenity` or `kdialog` is installed. On macOS, `osascript` is used automatically.
