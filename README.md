# sudo-me

**Agent-Native Transient Sudo**

`sudo-me` is a lightweight, secure, cross-platform utility written in Rust that allows AI coding agents (and humans) to execute commands with root privileges without needing to interactively enter a password for every command. 

It works by initializing a background daemon that authenticates via a cryptographically secure token over a Unix socket.

## Why?

When AI agents need to run commands that require root privileges (like `apt-get update`, `systemctl restart`, or editing system files), they often get stuck at the `sudo` password prompt. `sudo-me` solves this by prompting for the password exactly once (via a GUI dialog or TTY fallback) and then allowing subsequent commands to run seamlessly.

## Features

- **Passwordless Execution:** Run root commands without interactive prompts after a single initialization.
- **Root-Pinned TOFU (Trust On First Use):** Configuration is securely hashed and pinned by the root daemon to prevent user-space malware from silently downgrading security.
- **JIT Confirmation (Secure Mode):** Optional GUI/TTY prompts to confirm commands before execution, mitigating prompt injection attacks on AI agents.
- **Time-To-Live (TTL):** The background daemon automatically shuts down after a configurable period of inactivity.
- **Cross-Platform:** Works across Linux, macOS, and BSD using standard Unix sockets.
- **Agent Ready:** Includes a standard `SKILL.md` for seamless integration with AI coding agents.

## 🤖 For LLM Agents

If you want your AI agent to set this up automatically, just copy and paste this one-liner to your agent:

```bash
curl -fsSL https://raw.githubusercontent.com/AE-0/sudo-me/refs/heads/main/docs/guide/installation.md
```

## Installation

### Pre-compiled Binaries

You can download pre-compiled binaries for your platform from the [Releases page](https://github.com/AE-0/sudo-me/releases). Extract the archive and move the `sudo-me` binary to a directory in your `PATH` (e.g., `~/.local/bin`).

### Prerequisites (for building from source)
- Rust toolchain (or Podman/Docker for containerized building)
- A GUI dialog tool like `zenity` or `kdialog` (optional, for graphical password prompts)

### Building from Source

You can build `sudo-me` using standard Cargo:

```bash
cargo build --release
cp target/release/sudo-me ~/.local/bin/sudo-me
```

**Building with Podman (No Rust installation required):**

If you don't want to install Rust on your host system, you can build it using Podman:

```bash
podman run --rm -v "$(pwd):/app" -w /app docker.io/library/rust:latest cargo build --release
cp target/release/sudo-me ~/.local/bin/sudo-me
```

Make sure `~/.local/bin` is in your system's `PATH`.

## Usage

### 1. Initialize the Session

Before running any commands as root, initialize the session:

```bash
sudo-me init
```

This will prompt you for your sudo password once. The session details (socket path and secure token) are saved securely to `~/.sudo-me` (with `0600` permissions).

### 2. Run Commands

Once initialized, you can run commands as root from any shell:

```bash
sudo-me run apt-get update
sudo-me run systemctl restart nginx
sudo-me run whoami
```

## Installing the Agent Skill

`sudo-me` includes a standard Agent Skill (`SKILL.md`) compatible with the [Agent Skills ecosystem](https://skills.sh). This teaches your AI agent how to use `sudo-me` automatically.

You can install the skill to your preferred agent (e.g., OpenCode, Claude Code, Cursor) using the `skills` CLI:

```bash
# Install directly from GitHub
npx skills add AE-0/sudo-me

# Or install from a local clone
npx skills add ./sudo-me
```

## Security

- **Socket Permissions:** The Unix socket is created in a secure temporary directory (`/tmp/sudo-me-XXXXXX`) with `0700` permissions, owned by the user.
- **Token Authentication:** Connecting to the socket is not enough. Every IPC request must include a 32-character cryptographically secure random token generated during initialization.
- **Session Storage:** The session file (`~/.sudo-me`) is created with strict `0600` permissions.
- **Memory Safety:** Written in Rust, avoiding common memory vulnerabilities associated with C/C++ IPC daemons.

## License

GPL-3.0
