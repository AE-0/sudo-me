---
name: sudo-me
description: Use this skill to execute commands with root privileges securely without needing to interactively enter a password for every command. It uses a transient background daemon for authentication.
---

# Skill: sudo-me

# Agent-Native Transient Sudo (`sudo-me`)

This skill provides instructions on how to use `sudo-me` to execute commands with root privileges without needing to interactively enter a password for every command.

## Overview

`sudo-me` is a tool designed for AI agents to securely run commands as root. It works by initializing a background daemon that authenticates via a secure token over a Unix socket.

## Usage

### 1. Initialization

Before running any commands as root, you must initialize the `sudo-me` session. This will prompt for the sudo password once (via a GUI dialog or TTY fallback) and save the session securely.

Run the following command to initialize the session:

```bash
sudo-me init
```

*(Note: You can also run `eval $(sudo-me init)` if you want to export the variables directly into your current shell, but it is no longer required as the session is saved to `~/.sudo-me`.)*

### 2. Running Commands

Once initialized, you can run commands as root using `sudo-me run`.

```bash
sudo-me run <command> [args...]
```

**Examples:**

```bash
sudo-me run whoami
sudo-me run apt-get update
sudo-me run systemctl restart nginx
```

### 3. Important Notes

- The session is saved to `~/.sudo-me`. If you restart your machine or the daemon is killed, you will need to re-initialize by running `sudo-me init` again.
- Do not share the `SUDO_ME_TOKEN` or `SUDO_ME_SOCK` variables with untrusted processes.

## Troubleshooting

- **Error: Session not found. Please run `sudo-me init` first:**
  You haven't initialized the session. Run `sudo-me init` first.
- **Password prompt fails or hangs:**
  Ensure you have a GUI dialog tool installed (like `zenity` or `kdialog`) if running in a graphical environment, or that you are running the initialization in an interactive TTY.
- **Daemon error or connection refused:**
  The background daemon might have crashed or the socket file was deleted. Re-initialize the session by running `sudo-me init` again.
- **Command not found:**
  Ensure `sudo-me` is in your `PATH` (e.g., `~/.local/bin`).
