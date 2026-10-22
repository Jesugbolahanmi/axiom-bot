# 🚀 100% Free 24/7 Cloud Deployment Guide

This guide will keep your Axiom Solana Telegram Bot running **24/7 in the cloud for $0/month**, even when your PC is turned off or asleep.

---

## ⚡ Method 1: Fly.io (Fastest — 3 Minutes)

Fly.io gives you free 24/7 VMs that run containerized apps directly from a `Dockerfile`.

### Step 1: Install Fly CLI (Powershell)
Open PowerShell and run:
```powershell
iwr https://fly.io/install.ps1 -useb | iex
```

### Step 2: Sign Up / Log In (Free)
```powershell
fly auth signup
```
*(Or `fly auth login` if you already have an account)*

### Step 3: Set Your Bot Secrets (from your .env)
Inside `C:\Users\DELL\Desktop\axiom.edit`, run:
```powershell
fly secrets set TELEGRAM_BOT_TOKEN="your_token_from_env" TELEGRAM_ALLOWED_USER_ID=7363236725 HELIUS_RPC_URL="your_helius_url_from_env"
```

### Step 4: Deploy 24/7!
```powershell
fly launch --no-deploy
fly deploy
```

That's it! Your bot is now building in the cloud and will stay running 24/7 non-stop.

To check live logs from anywhere:
```powershell
fly logs
```

---

## 🖥️ Method 2: Oracle Cloud Always Free VPS (Permanent Linux Server)

If you prefer a permanent Ubuntu Linux server:
1. Create a free account at [oracle.com/cloud/free](https://www.oracle.com/cloud/free/)
2. Spin up an **Always Free Compute Instance** (Ubuntu 22.04 LTS).
3. SSH into your VPS:
   ```bash
   git clone <your-repo>
   cd axiom.edit
   cargo build --release
   ```
4. Run it as a persistent background `systemd` service:
   ```bash
   sudo nano /etc/systemd/system/axiom-bot.service
   ```
   Paste:
   ```ini
   [Unit]
   Description=Axiom Solana Bot
   After=network.target

   [Service]
   Type=simple
   User=ubuntu
   WorkingDirectory=/home/ubuntu/axiom.edit
   ExecStart=/home/ubuntu/axiom.edit/target/release/axiom-bot
   Restart=always
   RestartSec=5

   [Install]
   WantedBy=multi-user.target
   ```
   Enable and start:
   ```bash
   sudo systemctl enable --now axiom-bot
   ```
