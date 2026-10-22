# Solana New-Coin Filter & Alert Bot (Axiom Early Column Replica)

A high-performance Solana new-coin monitoring and alerting bot written in Rust. It captures newly launched tokens from Pump.fun (via PumpPortal WebSocket), evaluates them in an in-memory sliding window against configurable filter criteria matching Axiom's "Early" column, and sends rich Telegram alerts with interactive buy buttons.

---

## Features (Phase 1)

- **PumpPortal Real-Time Ingestion**: Connects via WebSockets with auto-reconnection and exponential backoff to stream new token creations and live trades.
- **Dynamic Trade Tracking**: Tracks sliding 5-minute volume and cumulative volume for every active token.
- **Continuous Watchlist Worker**: Tokens are held in an in-memory cache and evaluated periodically during their 5–60 minute window.
- **Configurable Filter Engine**:
  - Token Age: 5 to 60 minutes
  - Market Cap: $10,000 to $35,000
  - Total Volume: $\ge 1.0\times$ Market Cap
  - 5-Minute Volume: $\ge 10\%$ of Market Cap
  - Liquidity: $\ge 40\%$ of Market Cap
  - Holder Count: $\ge 50$
  - Dev Holding: $\le 15\%$
  - Top Holder: $\le 15\%$ (excluding bonding curve)
  - Sniper + Bundler: $\le 15\%$
  - Lore: Tags tokens as `[No Lore]` if empty or short; shows excerpt if present.
- **Telegram Alerting & Authorization**:
  - Rich HTML formatted alert with clickable links to **Axiom**, **DexScreener**, and **Solscan**.
  - Interactive inline keyboard: `[ 🛒 Buy 0.1 SOL ]`, `[ 🛒 Buy 0.5 SOL ]`, `[ ❌ Ignore ]`.
  - User ID whitelisting ensures only your Telegram account can interact with the bot.
  - Alert deduplication with configurable cooldown.
  - Built-in commands: `/status` and `/testalert`.

---

## Configuration

Copy `.env.example` to `.env`:

```bash
cp .env.example .env
```

Set your Telegram credentials in `.env`:
```env
TELEGRAM_BOT_TOKEN="your_bot_token_from_botfather"
TELEGRAM_ALLOWED_USER_ID=123456789
```

All filter thresholds and intervals can be modified without recompiling in `config.toml`:

```toml
[filters]
min_age_mins = 5
max_age_mins = 60
min_mcap_usd = 10000.0
max_mcap_usd = 35000.0
min_total_volume_mult = 1.0
min_5m_volume_ratio = 0.10
min_liquidity_ratio = 0.40
min_holders = 50
max_dev_holding_pct = 15.0
max_top_holder_pct = 15.0
max_sniper_bundler_pct = 15.0
min_lore_length = 20
```

---

## Running the Bot

```powershell
cargo run
```

Send `/status` or `/testalert` to your bot in Telegram to verify the connection and message styling.
