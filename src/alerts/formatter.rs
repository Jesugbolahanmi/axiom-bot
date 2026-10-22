use html_escape::encode_text;
use crate::models::alert::{AlertCard, AlertType, CallRecord, TopHolderItem};

fn abbreviate_address(addr: &str) -> String {
    if addr.len() > 10 {
        format!("{}...{}", &addr[..4], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}

fn format_amount(amount: f64) -> String {
    if amount >= 1_000_000_000.0 {
        format!("{:.2}B", amount / 1_000_000_000.0)
    } else if amount >= 1_000_000.0 {
        format!("{:.1}M", amount / 1_000_000.0)
    } else if amount >= 1_000.0 {
        format!("{:.1}K", amount / 1_000.0)
    } else {
        format!("{:.0}", amount)
    }
}

fn format_usd(usd: f64) -> String {
    if usd >= 1_000_000.0 {
        format!("~${:.2}M", usd / 1_000_000.0)
    } else if usd >= 1_000.0 {
        format!("~${:.2}K", usd / 1_000.0)
    } else {
        format!("~${:.0}", usd)
    }
}

fn format_top_holders_tree(holders: &[TopHolderItem], market_cap_usd: f64) -> String {
    if holders.is_empty() {
        return "├ Top Holders: Pending indexing\n".to_string();
    }

    let mut out = String::new();
    let display_count = holders.len().min(5);
    for (idx, h) in holders.iter().take(display_count).enumerate() {
        let is_last = idx == display_count - 1;
        let branch = if is_last { "└" } else { "├" };
        let short_addr = abbreviate_address(&h.address);
        let amt_str = format_amount(h.amount);
        let dev_flag = if h.is_dev { " [DEV]" } else { "" };
        let val_usd = (h.pct / 100.0) * market_cap_usd;
        let usd_str = format_usd(val_usd);

        out.push_str(&format!(
            "{} #{}: <code>{}</code>  {} (<b>{:.1}%</b>) {}{}\n",
            branch, h.rank, short_addr, amt_str, h.pct, usd_str, dev_flag
        ));
    }
    out
}

pub fn format_alert_html(card: &AlertCard) -> String {
    let name_escaped = encode_text(&card.name);
    let symbol_escaped = encode_text(&card.symbol);
    let mint = &card.mint;

    let lore_section = match &card.lore_description {
        Some(lore) if card.has_lore => {
            let truncated = if lore.len() > 180 {
                format!("{}...", &lore[..180])
            } else {
                lore.clone()
            };
            format!("<b>Lore</b>\n<i>\"{}\"</i>\n\n", encode_text(&truncated))
        }
        _ => String::new(),
    };

    let th_row = if !card.top_holders.is_empty() {
        card.top_holders
            .iter()
            .take(5)
            .map(|h| format!("{:.1}", h.pct))
            .collect::<Vec<_>>()
            .join("|")
    } else {
        format!("{:.1}", card.top_holder_pct)
    };

    let holders_tree = format_top_holders_tree(&card.top_holders, card.market_cap_usd);

    let cluster_summary = if card.clustered_wallets_count > 0 {
        format!("<b>{:.1}%</b> ({} wallets bundled in B0)", card.sniper_bundler_pct, card.clustered_wallets_count)
    } else {
        "None detected (Organic)".to_string()
    };

    let axiom_link = format!("https://axiom.trade/token/{}", mint);
    let dexscreener_link = format!("https://dexscreener.com/solana/{}", mint);
    let solscan_link = format!("https://solscan.io/token/{}", mint);
    let bubblemaps_link = format!("https://bubblemaps.io/solana/token/{}", mint);

    let mc_k = card.market_cap_usd / 1000.0;
    let liq_k = card.liquidity_usd / 1000.0;
    let vol_k = card.total_volume_usd / 1000.0;
    let vol5m_k = card.rolling_5m_volume_usd / 1000.0;

    let header = if card.alert_type == AlertType::Revival {
        format!("<b>{}</b> (<code>${}</code>) — <b>TOKEN REVIVAL</b>", name_escaped, symbol_escaped)
    } else {
        format!("<b>{}</b> (<code>${}</code>)", name_escaped, symbol_escaped)
    };

    let revival_block = if card.alert_type == AlertType::Revival {
        let bottom_k = card.bottom_market_cap_usd / 1000.0;
        let age_hours = card.age_mins / 60.0;
        format!(
            "<b>Revival Reclaim</b>\n\
            ├ Reclaim:     ${:.2}K ──&gt; <b>${:.2}K</b> (<b>+{:.1}%</b> from death)\n\
            ├ 5m Surge:    <b>${:.2}K</b> ({:.1}% MC)\n\
            ├ CTO Status:  <b>{}</b>\n\
            └ Age:         <b>{:.1}h</b> (Dormant Re-awakening)\n\n",
            bottom_k,
            mc_k,
            card.surge_pct_from_bottom,
            vol5m_k,
            card.volume_5m_to_mc_ratio * 100.0,
            card.cto_status_desc,
            age_hours
        )
    } else {
        String::new()
    };

    format!(
        "{}\n\
        <code>{}</code>\n\n\
        {}\
        <b>Token Stats</b>\n\
        ├ MC:   <b>${:.2}K</b>\n\
        ├ LIQ:  <b>${:.2}K</b> ({:.1}% MC)\n\
        ├ VOL:  <b>${:.2}K</b> (5m: ${:.2}K)\n\
        ├ HLD:  <b>{}</b>\n\
        └ Age:  <b>{:.1}m</b>\n\n\
        <b>Security &amp; Wallet Clusters</b>\n\
        ├ Dev Holding: <b>{:.1}%</b>\n\
        ├ Top 10:      <b>{:.1}%</b> | {} (total)\n\
        ├ TH:          <code>{}</code>\n\
        ├ Clustered:   {}\n\
        └ Bubblemaps:  <a href=\"{}\">Inspect Wallet Connections ↗</a>\n\n\
        <b>Top Holders (excl. curve)</b>\n\
        {}\n\
        {}\
        <a href=\"{}\">Axiom</a> • <a href=\"{}\">DexScreener</a> • <a href=\"{}\">Bubblemaps</a> • <a href=\"{}\">Solscan</a>",
        header,
        mint,
        revival_block,
        mc_k,
        liq_k,
        card.liquidity_ratio * 100.0,
        vol_k,
        vol5m_k,
        card.holder_count,
        card.age_mins,
        card.dev_holding_pct,
        card.top_10_pct,
        card.holder_count,
        th_row,
        cluster_summary,
        bubblemaps_link,
        holders_tree,
        lore_section,
        axiom_link,
        dexscreener_link,
        bubblemaps_link,
        solscan_link
    )
}

pub fn format_refreshed_alert_html(record: &CallRecord) -> String {
    let name_escaped = encode_text(&record.name);
    let symbol_escaped = encode_text(&record.symbol);
    let mint = &record.mint;

    let multiple = if record.initial_market_cap_usd > 0.0 {
        record.current_market_cap_usd / record.initial_market_cap_usd
    } else {
        1.0
    };
    let ath_multiple = if record.initial_market_cap_usd > 0.0 {
        record.highest_market_cap_usd / record.initial_market_cap_usd
    } else {
        1.0
    };
    let gain_pct = (multiple - 1.0) * 100.0;
    let gain_sign = if gain_pct >= 0.0 { "+" } else { "" };
    let return_badge = if multiple >= 5.0 {
        "<b>[MOONSHOT 5X+]</b>"
    } else if multiple >= 2.0 {
        "<b>[2X+ FLIP]</b>"
    } else if multiple >= 1.0 {
        "<b>[IN PROFIT]</b>"
    } else {
        "<b>[UNDER CALL]</b>"
    };

    let lore_section = match &record.lore_description {
        Some(lore) if !lore.is_empty() => {
            let truncated = if lore.len() > 180 {
                format!("{}...", &lore[..180])
            } else {
                lore.clone()
            };
            format!("<b>Lore</b>\n<i>\"{}\"</i>\n\n", encode_text(&truncated))
        }
        _ => String::new(),
    };

    let elapsed_mins = (chrono::Utc::now() - record.alert_timestamp).num_seconds() as f64 / 60.0;
    let liq_pct = if record.current_market_cap_usd > 0.0 {
        (record.current_liquidity_usd / record.current_market_cap_usd) * 100.0
    } else {
        0.0
    };

    let th_row = if !record.top_holders.is_empty() {
        record.top_holders
            .iter()
            .take(5)
            .map(|h| format!("{:.1}", h.pct))
            .collect::<Vec<_>>()
            .join("|")
    } else {
        format!("{:.1}", record.top_holder_pct)
    };

    let holders_tree = format_top_holders_tree(&record.top_holders, record.current_market_cap_usd);

    let cluster_summary = if record.clustered_wallets_count > 0 {
        format!("<b>{:.1}%</b> ({} wallets bundled in B0)", record.sniper_bundler_pct, record.clustered_wallets_count)
    } else {
        "None detected (Organic)".to_string()
    };

    let axiom_link = format!("https://axiom.trade/token/{}", mint);
    let dexscreener_link = format!("https://dexscreener.com/solana/{}", mint);
    let solscan_link = format!("https://solscan.io/token/{}", mint);
    let bubblemaps_link = format!("https://bubblemaps.io/solana/token/{}", mint);

    let mc_k = record.current_market_cap_usd / 1000.0;
    let call_mc_k = record.initial_market_cap_usd / 1000.0;
    let liq_k = record.current_liquidity_usd / 1000.0;
    let vol_k = record.current_volume_usd / 1000.0;
    let vol5m_k = record.current_volume_5m_usd / 1000.0;

    let revival_badge = if record.alert_type == AlertType::Revival {
        " [REVIVAL]"
    } else {
        ""
    };

    format!(
        "<b>{}</b> (<code>${}</code>){} — <b>LIVE REFRESH</b>\n\
        <code>{}</code>\n\n\
        <b>Performance</b>\n\
        ├ Status:  {}\n\
        ├ Return:  <b>{:.2}x</b> ({}{:.1}%)\n\
        ├ ATH:     <b>{:.2}x</b>\n\
        └ MCap:    ${:.1}K —&gt; <b>${:.1}K</b>\n\n\
        <b>Live Stats</b>\n\
        ├ LIQ:  <b>${:.2}K</b> ({:.1}% MC)\n\
        ├ VOL:  <b>${:.2}K</b> (5m: ${:.2}K)\n\
        ├ HLD:  <b>{}</b>\n\
        └ Age:  <b>{:.1}m</b> (Alerted {:.1}m ago)\n\n\
        <b>Security &amp; Wallet Clusters</b>\n\
        ├ Dev Holding: <b>{:.1}%</b>\n\
        ├ Top 10:      <b>{:.1}%</b> | {} (total)\n\
        ├ TH:          <code>{}</code>\n\
        ├ Clustered:   {}\n\
        └ Bubblemaps:  <a href=\"{}\">Inspect Wallet Connections ↗</a>\n\n\
        <b>Top Holders (excl. curve)</b>\n\
        {}\n\
        {}\
        <a href=\"{}\">Axiom</a> • <a href=\"{}\">DexScreener</a> • <a href=\"{}\">Bubblemaps</a> • <a href=\"{}\">Solscan</a>\n\
        <i>Refreshed at {}</i>",
        name_escaped,
        symbol_escaped,
        revival_badge,
        mint,
        return_badge,
        multiple,
        gain_sign,
        gain_pct,
        ath_multiple,
        call_mc_k,
        mc_k,
        liq_k,
        liq_pct,
        vol_k,
        vol5m_k,
        record.holder_count,
        elapsed_mins,
        elapsed_mins,
        record.dev_holding_pct,
        record.top_10_pct,
        record.holder_count,
        th_row,
        cluster_summary,
        bubblemaps_link,
        holders_tree,
        lore_section,
        axiom_link,
        dexscreener_link,
        bubblemaps_link,
        solscan_link,
        record.last_refreshed.format("%H:%M:%S UTC")
    )
}

pub fn format_calls_summary(calls: &[CallRecord]) -> String {
    if calls.is_empty() {
        return "<b>Axiom Calls &amp; Flips Tracker</b>\n\n\
                <i>No alerted calls recorded yet. Watcher is tracking Pump.fun launches in real-time.</i>".to_string();
    }

    let mut out = String::from("<b>Axiom Calls &amp; Flips Tracker (Xs)</b>\n\n");
    for (i, call) in calls.iter().enumerate().take(15) {
        let multiple = if call.initial_market_cap_usd > 0.0 {
            call.current_market_cap_usd / call.initial_market_cap_usd
        } else {
            1.0
        };
        let ath_multiple = if call.initial_market_cap_usd > 0.0 {
            call.highest_market_cap_usd / call.initial_market_cap_usd
        } else {
            1.0
        };
        let gain_pct = (multiple - 1.0) * 100.0;
        let gain_sign = if gain_pct >= 0.0 { "+" } else { "" };
        let elapsed_mins = (chrono::Utc::now() - call.alert_timestamp).num_seconds() as f64 / 60.0;

        let short_mint = if call.mint.len() > 10 {
            format!("{}...{}", &call.mint[..4], &call.mint[call.mint.len() - 4..])
        } else {
            call.mint.clone()
        };

        let call_mc = call.initial_market_cap_usd / 1000.0;
        let now_mc = call.current_market_cap_usd / 1000.0;

        let tag = if call.alert_type == AlertType::Revival { " [REVIVAL]" } else { "" };

        out.push_str(&format!(
            "{}. <b>${}</b>{} (<code>{}</code>)\n\
            ├ Return: <b>{:.2}x</b> ({}{:.0}%) | ATH: <b>{:.2}x</b>\n\
            ├ MCap:   ${:.1}K —&gt; <b>${:.1}K</b>\n\
            └ Alerted {:.1}m ago • <a href=\"https://dexscreener.com/solana/{}\">DexScreener</a> • <a href=\"https://bubblemaps.io/solana/token/{}\">Bubblemaps</a>\n\n",
            i + 1,
            encode_text(&call.symbol),
            tag,
            short_mint,
            multiple,
            gain_sign,
            gain_pct,
            ath_multiple,
            call_mc,
            now_mc,
            elapsed_mins,
            call.mint,
            call.mint
        ));
    }

    out.push_str(&format!("<i>Total Tracked: {}</i>", calls.len()));
    out
}
