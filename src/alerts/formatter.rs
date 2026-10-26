use html_escape::encode_text;
use crate::models::alert::{AlertCard, AlertType, CallRecord, TopHolderItem};

fn abbreviate_address(addr: &str) -> String {
    if addr.len() > 10 {
        format!("{}...{}", &addr[..4], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}

/// Intelligent financial formatter:
/// - >= $1B  => $1.54B
/// - >= $100M => $154.0M
/// - >= $1M   => $30.80M / $1.25M
/// - >= $1K   => $310.7K / $24.5K
/// - < $1K    => $450
pub fn format_smart_usd(v: f64) -> String {
    if v >= 1_000_000_000.0 {
        format!("${:.2}B", v / 1_000_000_000.0)
    } else if v >= 100_000_000.0 {
        format!("${:.1}M", v / 1_000_000.0)
    } else if v >= 1_000_000.0 {
        format!("${:.2}M", v / 1_000_000.0)
    } else if v >= 1_000.0 {
        format!("${:.1}K", v / 1_000.0)
    } else if v > 0.0 {
        format!("${:.0}", v)
    } else {
        "$0".to_string()
    }
}

pub fn format_count(c: u64) -> String {
    if c >= 1_000_000 {
        format!("{:.1}M", c as f64 / 1_000_000.0)
    } else if c >= 10_000 {
        format!("{:.1}K", c as f64 / 1_000.0)
    } else {
        c.to_string()
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
    if usd >= 1_000_000_000.0 {
        format!("~${:.2}B", usd / 1_000_000_000.0)
    } else if usd >= 1_000_000.0 {
        format!("~${:.2}M", usd / 1_000_000.0)
    } else if usd >= 1_000.0 {
        format!("~${:.1}K", usd / 1_000.0)
    } else {
        format!("~${:.0}", usd)
    }
}

pub fn format_top_holders_tree(holders: &[TopHolderItem], market_cap_usd: f64) -> String {
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
            format!("<b>Lore</b>
<i>\"{}\"</i>\n\n", encode_text(&truncated))
        }
        _ => String::new(),
    };

    let axiom_link = format!("https://axiom.trade/token/{}", mint);
    let bm_chain = match card.chain.as_str() { "base" => "base", "bsc" => "bsc", "ethereum" => "eth", _ => "solana" };
    let bubblemaps_link = format!("https://bubblemaps.io/{}/token/{}", bm_chain, mint);

    let mc_str = format_smart_usd(card.market_cap_usd);
    let liq_str = format_smart_usd(card.liquidity_usd);
    let vol_str = format_smart_usd(card.total_volume_usd);
    let vol5m_str = format_smart_usd(card.rolling_5m_volume_usd);

    let age_str = if card.age_mins >= 1440.0 {
        format!("{:.1}d", card.age_mins / 1440.0)
    } else if card.age_mins >= 60.0 {
        format!("{:.1}h", card.age_mins / 60.0)
    } else {
        format!("{:.0}m", card.age_mins)
    };

    let chain_uc = card.chain.to_uppercase();
    let header = if card.alert_type == AlertType::Revival || card.alert_type == AlertType::MultiChainRevival {
        format!("🔄 <b>TOKEN REVIVAL</b> • <b>{}</b>\n<b>{}</b> (<code>${}</code>)", chain_uc, name_escaped, symbol_escaped)
    } else {
        format!("🚨 <b>NEW EARLY LAUNCH</b> • <b>{}</b>\n<b>{}</b> (<code>${}</code>)", chain_uc, name_escaped, symbol_escaped)
    };

    let revival_block = if card.alert_type == AlertType::Revival || card.alert_type == AlertType::MultiChainRevival {
        if card.bottom_market_cap_usd > 0.0 {
            let bottom_str = format_smart_usd(card.bottom_market_cap_usd);
            format!(
                "<b>Revival Reclaim</b>\n                ├ Bottom:  {} ──&gt; <b>{}</b> (<b>+{:.0}%</b>)\n                ├ 5m Flow: <b>{}</b>\n                └ Status:  <b>{}</b>\n\n",
                bottom_str,
                mc_str,
                card.surge_pct_from_bottom,
                vol5m_str,
                card.cto_status_desc
            )
        } else {
            format!(
                "<b>Revival Flow</b>\n                ├ 5m Vol:  <b>{}</b> (Sustained)\n                └ Status:  <b>{}</b>\n\n",
                vol5m_str,
                card.cto_status_desc
            )
        }
    } else {
        String::new()
    };

    // Socials section
    let mut social_links: Vec<String> = Vec::new();
    for w in card.websites.iter().filter(|u| u.starts_with("http")).take(2) {
        social_links.push(format!("<a href=\"{}\">Website</a>", html_escape::encode_double_quoted_attribute(w)));
    }
    for (kind, url) in card.socials.iter().filter(|(_, u)| u.starts_with("http")) {
        let label = match kind.as_str() {
            "twitter" => "X".to_string(),
            "telegram" => "Telegram".to_string(),
            "discord" => "Discord".to_string(),
            other => encode_text(other).to_string(),
        };
        social_links.push(format!("<a href=\"{}\">{}</a>", html_escape::encode_double_quoted_attribute(url), label));
    }
    let socials_line = if !social_links.is_empty() {
        format!("<b>Socials:</b> {}\n\n", social_links.join(" • "))
    } else {
        String::new()
    };

    let links_line = if card.chain == "solana" {
        format!("<a href=\"{}\">Axiom</a> • <a href=\"https://dexscreener.com/solana/{}\">DexScreener</a> • <a href=\"{}\">Bubblemaps</a> • <a href=\"https://solscan.io/token/{}\">Solscan</a>", axiom_link, mint, bubblemaps_link, mint)
    } else {
        format!("<a href=\"https://dexscreener.com/{}/{}\">DexScreener</a> • <a href=\"{}\">Bubblemaps</a>", card.chain, mint, bubblemaps_link)
    };

    format!(
        "{}\n        <code>{}</code>\n\n        {}        <b>Market Stats</b>\n        ├ MCap: <b>{}</b>\n        ├ LIQ:  <b>{}</b> ({:.1}%)\n        ├ VOL:  <b>{}</b> (5m: {})\n        ├ HLD:  <b>{}</b>\n        └ Age:  <b>{}</b>\n\n        {}        {}        {}",
        header,
        mint,
        revival_block,
        mc_str,
        liq_str,
        card.liquidity_ratio * 100.0,
        vol_str,
        vol5m_str,
        format_count(card.holder_count),
        age_str,
        lore_section,
        socials_line,
        links_line
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
        "[MOONSHOT 5X+]"
    } else if multiple >= 2.0 {
        "[2X+ FLIP]"
    } else if multiple >= 1.0 {
        "[IN PROFIT]"
    } else {
        "[UNDER CALL]"
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
    let age_str = if elapsed_mins >= 1440.0 {
        format!("{:.1}d", elapsed_mins / 1440.0)
    } else if elapsed_mins >= 60.0 {
        format!("{:.1}h", elapsed_mins / 60.0)
    } else {
        format!("{:.0}m", elapsed_mins)
    };

    let liq_pct = if record.current_market_cap_usd > 0.0 {
        (record.current_liquidity_usd / record.current_market_cap_usd) * 100.0
    } else {
        0.0
    };

    let mc_str = format_smart_usd(record.current_market_cap_usd);
    let call_mc_str = format_smart_usd(record.initial_market_cap_usd);
    let ath_mc_str = format_smart_usd(record.highest_market_cap_usd);
    let liq_str = format_smart_usd(record.current_liquidity_usd);
    let vol_str = format_smart_usd(record.current_volume_usd);
    let vol5m_str = format_smart_usd(record.current_volume_5m_usd);

    let chain_uc = record.chain.to_uppercase();
    let revival_badge = match record.alert_type {
        AlertType::Revival => " • <b>REVIVAL</b>".to_string(),
        AlertType::MultiChainRevival => format!(" • <b>{}</b>", chain_uc),
        _ => format!(" • <b>{}</b>", chain_uc),
    };

    // Socials section
    let mut social_links: Vec<String> = Vec::new();
    for w in record.websites.iter().filter(|u| u.starts_with("http")).take(2) {
        social_links.push(format!("<a href=\"{}\">Website</a>", html_escape::encode_double_quoted_attribute(w)));
    }
    for (kind, url) in record.socials.iter().filter(|(_, u)| u.starts_with("http")) {
        let label = match kind.as_str() {
            "twitter" => "X".to_string(),
            "telegram" => "Telegram".to_string(),
            "discord" => "Discord".to_string(),
            other => encode_text(other).to_string(),
        };
        social_links.push(format!("<a href=\"{}\">{}</a>", html_escape::encode_double_quoted_attribute(url), label));
    }
    let socials_line = if !social_links.is_empty() {
        format!("<b>Socials:</b> {}\n\n", social_links.join(" • "))
    } else {
        String::new()
    };

    // Quick links
    let axiom_link = format!("https://axiom.trade/token/{}", mint);
    let bm_chain = match record.chain.as_str() { "base" => "base", "bsc" => "bsc", "ethereum" => "eth", _ => "solana" };
    let bubblemaps_link = format!("https://bubblemaps.io/{}/token/{}", bm_chain, mint);
    let links_line = if record.chain == "solana" {
        format!("<a href=\"{}\">Axiom</a> • <a href=\"https://dexscreener.com/solana/{}\">DexScreener</a> • <a href=\"{}\">Bubblemaps</a> • <a href=\"https://solscan.io/token/{}\">Solscan</a>", axiom_link, mint, bubblemaps_link, mint)
    } else {
        format!("<a href=\"https://dexscreener.com/{}/{}\">DexScreener</a> • <a href=\"{}\">Bubblemaps</a>", record.chain, mint, bubblemaps_link)
    };

    format!(
        "<b>{}</b> (<code>${}</code>){} — <b>LIVE REFRESH</b>\n        <code>{}</code>\n\n        <b>Call Performance</b>\n        ├ Status:    <b>{}</b>\n        ├ Call MCap: <b>{}</b>\n        ├ Peak ATH:  <b>{:.2}x</b> ({})\n        └ Current:   <b>{:.2}x</b> ({}, {}{:.1}%)\n\n        <b>Live Stats</b>\n        ├ MCap: <b>{}</b>\n        ├ LIQ:  <b>{}</b> ({:.1}%)\n        ├ VOL:  <b>{}</b> (5m: {})\n        ├ HLD:  <b>{}</b>\n        └ Age:  <b>{}</b>\n\n        {}        {}        {}\n        <i>Refreshed at {}</i>",
        name_escaped,
        symbol_escaped,
        revival_badge,
        mint,
        return_badge,
        call_mc_str,
        ath_multiple,
        ath_mc_str,
        multiple,
        mc_str,
        gain_sign,
        gain_pct,
        mc_str,
        liq_str,
        liq_pct,
        vol_str,
        vol5m_str,
        format_count(record.holder_count),
        age_str,
        lore_section,
        socials_line,
        links_line,
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

        let call_mc_str = format_smart_usd(call.initial_market_cap_usd);
        let now_mc_str = format_smart_usd(call.current_market_cap_usd);

        let tag = match call.alert_type {
            AlertType::Revival => " [REVIVAL]".to_string(),
            AlertType::MultiChainRevival => format!(" [{}]", call.chain.to_uppercase()),
            _ => String::new(),
        };

        out.push_str(&format!(
            "{}. <b>${}</b>{} (<code>{}</code>)\n\
            ├ Return: <b>{:.2}x</b> ({}{:.0}%) | ATH: <b>{:.2}x</b>\n\
            ├ MCap:   {} —&gt; <b>{}</b>\n\
            └ Alerted {:.1}m ago • <a href=\"https://dexscreener.com/solana/{}\">DexScreener</a> • <a href=\"https://bubblemaps.io/solana/token/{}\">Bubblemaps</a>\n\n",
            i + 1,
            encode_text(&call.symbol),
            tag,
            short_mint,
            multiple,
            gain_sign,
            gain_pct,
            ath_multiple,
            call_mc_str,
            now_mc_str,
            elapsed_mins,
            call.mint,
            call.mint
        ));
    }

    out.push_str(&format!("<i>Total Tracked: {}</i>", calls.len()));
    out
}


pub fn format_pnl_analytics(calls: &[CallRecord], timeframe_days: Option<u32>) -> String {
    let period_label = match timeframe_days {
        Some(1) => "Past 24 Hours",
        Some(7) => "Past 7 Days",
        Some(30) => "Past 30 Days",
        _ => "All Time",
    };

    let cutoff = timeframe_days.map(|d| chrono::Utc::now() - chrono::Duration::days(d as i64));
    let period_calls: Vec<&CallRecord> = calls
        .iter()
        .filter(|c| {
            if let Some(cut) = cutoff {
                c.alert_timestamp >= cut
            } else {
                true
            }
        })
        .collect();

    if period_calls.is_empty() {
        return format!(
            "📊 <b>Axiom Calls Performance &amp; PnL ({})</b>\n\n            <i>No alerted calls recorded in this timeframe yet.\n            The automated background watcher tracks every call's live ATH and PnL 24/7.</i>",
            period_label
        );
    }

    let total = period_calls.len();
    let mut wins = 0;
    let mut bucket_10x = 0;
    let mut bucket_5x = 0;
    let mut bucket_2x = 0;
    let mut bucket_1x = 0;
    let mut bucket_loss = 0;

    let mut sum_ath_multiple = 0.0;
    let mut sum_curr_multiple = 0.0;
    let mut best_call: Option<&CallRecord> = None;
    let mut best_multiple = 0.0;

    // Simulated 0.10 SOL fixed entry per call
    let fixed_entry_sol = 0.10;
    let total_invested_sol = total as f64 * fixed_entry_sol;
    let mut total_peak_return_sol = 0.0;
    let mut total_curr_return_sol = 0.0;

    for call in &period_calls {
        let ath_mult = if call.initial_market_cap_usd > 0.0 {
            call.highest_market_cap_usd / call.initial_market_cap_usd
        } else {
            1.0
        };
        let curr_mult = if call.initial_market_cap_usd > 0.0 {
            call.current_market_cap_usd / call.initial_market_cap_usd
        } else {
            1.0
        };

        sum_ath_multiple += ath_mult;
        sum_curr_multiple += curr_mult;

        total_peak_return_sol += ath_mult * fixed_entry_sol;
        total_curr_return_sol += curr_mult * fixed_entry_sol;

        if ath_mult >= 2.0 {
            wins += 1;
        }

        if ath_mult >= 10.0 {
            bucket_10x += 1;
        } else if ath_mult >= 5.0 {
            bucket_5x += 1;
        } else if ath_mult >= 2.0 {
            bucket_2x += 1;
        } else if ath_mult >= 1.0 {
            bucket_1x += 1;
        } else {
            bucket_loss += 1;
        }

        if ath_mult > best_multiple {
            best_multiple = ath_mult;
            best_call = Some(call);
        }
    }

    let win_rate = (wins as f64 / total as f64) * 100.0;
    let avg_ath = sum_ath_multiple / total as f64;
    let avg_curr = sum_curr_multiple / total as f64;

    let peak_pnl_sol = total_peak_return_sol - total_invested_sol;
    let peak_roi_pct = if total_invested_sol > 0.0 { (peak_pnl_sol / total_invested_sol) * 100.0 } else { 0.0 };

    let curr_pnl_sol = total_curr_return_sol - total_invested_sol;
    let curr_roi_pct = if total_invested_sol > 0.0 { (curr_pnl_sol / total_invested_sol) * 100.0 } else { 0.0 };
    let curr_sign = if curr_pnl_sol >= 0.0 { "+" } else { "" };

    let top_runner_str = if let Some(top) = best_call {
        format!("<b>${}</b> (<b>{:.2}x</b> ATH)", encode_text(&top.symbol), best_multiple)
    } else {
        "N/A".to_string()
    };

    let pct_10x = (bucket_10x as f64 / total as f64) * 100.0;
    let pct_5x = (bucket_5x as f64 / total as f64) * 100.0;
    let pct_2x = (bucket_2x as f64 / total as f64) * 100.0;
    let pct_1x = (bucket_1x as f64 / total as f64) * 100.0;
    let pct_loss = (bucket_loss as f64 / total as f64) * 100.0;

    format!(
        "📊 <b>Axiom Calls Performance &amp; PnL ({})</b>\n\n        <b>Overview</b>\n        ├ Total Calls:    <b>{}</b>\n        ├ Win Rate (≥2x): <b>{:.1}%</b> ({}/{} profitable)\n        ├ Average Peak:   <b>{:.2}x</b> ATH\n        ├ Average Live:   <b>{:.2}x</b> current\n        └ Top Performer:  {}\n\n        <b>Multipliers Breakdown</b>\n        ├ 🚀 10x+  Moonshots: <b>{}</b> ({:.1}%)\n        ├ 🔥 5x-10x Surges:   <b>{}</b> ({:.1}%)\n        ├ 🟢 2x-5x  Wins:     <b>{}</b> ({:.1}%)\n        ├ ⚪ 1x-2x  Modest:   <b>{}</b> ({:.1}%)\n        └ 🔴 &lt; 1x  Drawdown: <b>{}</b> ({:.1}%)\n\n        <b>Simulated PnL (0.10 SOL Entry / Call)</b>\n        ├ Capital Deployed: <b>{:.2} SOL</b>\n        ├ Peak Value Out:   <b>{:.2} SOL</b> (<b>+{:.2} SOL</b> | +{:.0}%)\n        └ Current Portfolio: <b>{:.2} SOL</b> (<b>{}{:.2} SOL</b> | {}{:.0}%)\n\n        <i>Tap buttons below to change timeframe:</i>",
        period_label,
        total,
        win_rate,
        wins,
        total,
        avg_ath,
        avg_curr,
        top_runner_str,
        bucket_10x,
        pct_10x,
        bucket_5x,
        pct_5x,
        bucket_2x,
        pct_2x,
        bucket_1x,
        pct_1x,
        bucket_loss,
        pct_loss,
        total_invested_sol,
        total_peak_return_sol,
        peak_pnl_sol,
        peak_roi_pct,
        total_curr_return_sol,
        curr_sign,
        curr_pnl_sol,
        curr_sign,
        curr_roi_pct
    )
}

pub fn format_milestone_alert(call: &CallRecord, milestone: f64) -> String {
    let ath_multiple = if call.initial_market_cap_usd > 0.0 {
        call.highest_market_cap_usd / call.initial_market_cap_usd
    } else {
        milestone
    };
    let curr_multiple = if call.initial_market_cap_usd > 0.0 {
        call.current_market_cap_usd / call.initial_market_cap_usd
    } else {
        1.0
    };
    let call_mc_str = format_smart_usd(call.initial_market_cap_usd);
    let ath_mc_str = format_smart_usd(call.highest_market_cap_usd);
    let curr_mc_str = format_smart_usd(call.current_market_cap_usd);
    let elapsed_mins = (chrono::Utc::now() - call.alert_timestamp).num_seconds() as f64 / 60.0;

    let short_mint = if call.mint.len() > 10 {
        format!("{}...{}", &call.mint[..4], &call.mint[call.mint.len() - 4..])
    } else {
        call.mint.clone()
    };

    let tag = match call.alert_type {
            AlertType::Revival => " [REVIVAL]".to_string(),
            AlertType::MultiChainRevival => format!(" [{}]", call.chain.to_uppercase()),
            _ => String::new(),
        };

    format!(
        "🎯 <b>CALL ATH UPDATE: ${}{} ({:.0}x Milestone Reached!)</b>\n\n        <b>Performance Metrics</b>\n        ├ Peak ATH:  <b>{:.2}x</b> ({} —&gt; <b>{}</b>)\n        ├ Current:   <b>{:.2}x</b> ({})\n        ├ Mint:      <code>{}</code>\n        └ Alerted:   <b>{:.1}m ago</b>\n\n        <a href=\"https://dexscreener.com/solana/{}\">DexScreener</a> • <a href=\"https://bubblemaps.io/solana/token/{}\">Bubblemaps</a> • <a href=\"https://axiom.trade/token/{}\">Axiom</a>",
        encode_text(&call.symbol),
        tag,
        milestone,
        ath_multiple,
        call_mc_str,
        ath_mc_str,
        curr_multiple,
        curr_mc_str,
        short_mint,
        elapsed_mins,
        call.mint,
        call.mint,
        call.mint
    )
}


pub fn format_token_pnl_html(record: &CallRecord) -> String {
    let sym = encode_text(&record.symbol);
    let name = encode_text(&record.name);
    let init_mc_str = format_smart_usd(record.initial_market_cap_usd);
    let curr_mc_str = format_smart_usd(record.current_market_cap_usd);
    let highest_mc_str = format_smart_usd(record.highest_market_cap_usd);

    let curr_mult = if record.initial_market_cap_usd > 0.0 {
        record.current_market_cap_usd / record.initial_market_cap_usd
    } else {
        1.0
    };
    let peak_mult = if record.initial_market_cap_usd > 0.0 {
        record.highest_market_cap_usd / record.initial_market_cap_usd
    } else {
        1.0
    };

    let elapsed_mins = (chrono::Utc::now() - record.alert_timestamp).num_seconds() as f64 / 60.0;
    let elapsed_str = if elapsed_mins >= 1440.0 {
        format!("{:.1}d ago", elapsed_mins / 1440.0)
    } else if elapsed_mins >= 60.0 {
        format!("{:.1}h ago", elapsed_mins / 60.0)
    } else if elapsed_mins < 1.0 {
        "just now".to_string()
    } else {
        format!("{:.0}m ago", elapsed_mins)
    };

    let badge = if peak_mult >= 10.0 {
        " 🚀 [10x+ MOON]"
    } else if peak_mult >= 5.0 {
        " 🔥 [5x+ SURGE]"
    } else if peak_mult >= 2.0 {
        " 🎯 [2x+ WIN]"
    } else if peak_mult >= 1.0 {
        " 📈 [IN PROFIT]"
    } else {
        " 🔻 [LOSS]"
    };

    format!(
        "🎯 <b>${}</b> ({}){}

        ├ <b>Call MCap:</b>  {}
        ├ <b>Peak ATH:</b>   <b>{:.2}x</b> ({})
        ├ <b>Current:</b>    <b>{:.2}x</b> ({})
        └ <b>Alerted:</b>    {}

        <a href=\"https://dexscreener.com/solana/{}\">DexScreener</a> • <a href=\"https://axiom.trade/token/{}\">Axiom</a>",
        sym,
        name,
        badge,
        init_mc_str,
        peak_mult,
        highest_mc_str,
        curr_mult,
        curr_mc_str,
        elapsed_str,
        record.mint,
        record.mint
    )
}


fn plain_usd(v: f64) -> String {
    format_smart_usd(v)
}

fn format_price(p: f64) -> String {
    if p <= 0.0 {
        return "n/a".to_string();
    }
    if p >= 1.0 {
        return format!("${:.4}", p);
    }
    let decimals = ((-p.log10()).ceil() as usize + 3).min(14);
    format!("${:.*}", decimals, p)
}

fn format_age_mins(mins: f64) -> String {
    if mins >= 1440.0 {
        format!("{:.1}d", mins / 1440.0)
    } else if mins >= 60.0 {
        format!("{:.1}h", mins / 60.0)
    } else {
        format!("{:.0}m", mins)
    }
}

fn signed_pct(v: f64) -> String {
    format!("{}{:.1}%", if v >= 0.0 { "+" } else { "" }, v)
}

/// Live coin details card for any pasted contract address (all DexScreener chains).
pub fn format_token_details_html(
    ov: &crate::enrichment::dex_screener::TokenOverview,
    holders: Option<&crate::enrichment::helius_das::TokenMetrics>,
    call: Option<&CallRecord>,
) -> String {
    let chain_uc = ov.chain.to_uppercase();
    let addr = &ov.token_address;
    let mut out = String::new();

    out.push_str(&format!(
        "🔎 <b>{}</b> (<code>${}</code>) • <b>{}</b>\n<code>{}</code>\n\n",
        encode_text(&ov.name),
        encode_text(&ov.symbol),
        encode_text(&chain_uc),
        addr
    ));

    // Market
    let age = ov
        .pair_created_at_ms
        .map(|ms| format_age_mins((chrono::Utc::now().timestamp_millis() - ms).max(0) as f64 / 60_000.0))
        .unwrap_or_else(|| "unknown".to_string());
    out.push_str("<b>Market</b>\n");
    out.push_str(&format!("├ Price: <b>{}</b>\n", format_price(ov.price_usd)));
    out.push_str(&format!("├ MC:    <b>{}</b> (FDV {})\n", plain_usd(ov.market_cap_usd), plain_usd(ov.fdv_usd)));
    if ov.liquidity_usd > 0.0 {
        let liq_pct = if ov.market_cap_usd > 0.0 { ov.liquidity_usd / ov.market_cap_usd * 100.0 } else { 0.0 };
        out.push_str(&format!("├ LIQ:   <b>{}</b> ({:.1}% MC)\n", plain_usd(ov.liquidity_usd), liq_pct));
    } else {
        out.push_str("├ LIQ:   <i>bonding curve / not reported</i>\n");
    }
    out.push_str(&format!("├ Age:   <b>{}</b>\n", age));
    out.push_str(&format!(
        "└ DEX:   {} ({} pair{})\n\n",
        encode_text(&ov.dex_id),
        ov.pair_count,
        if ov.pair_count == 1 { "" } else { "s" }
    ));

    // Price change
    out.push_str(&format!(
        "<b>Price Change</b>\n5m <b>{}</b> │ 1h <b>{}</b> │ 6h <b>{}</b> │ 24h <b>{}</b>\n\n",
        signed_pct(ov.chg_5m),
        signed_pct(ov.chg_1h),
        signed_pct(ov.chg_6h),
        signed_pct(ov.chg_24h)
    ));

    // Volume & flow
    let buy_pct = |b: u64, s: u64| if b + s > 0 { b as f64 / (b + s) as f64 * 100.0 } else { 0.0 };
    out.push_str("<b>Volume &amp; Flow</b>\n");
    out.push_str(&format!(
        "├ 5m:  <b>{}</b> — {} buys / {} sells ({:.0}% buys)\n",
        plain_usd(ov.vol_5m), ov.buys_5m, ov.sells_5m, buy_pct(ov.buys_5m, ov.sells_5m)
    ));
    out.push_str(&format!(
        "├ 1h:  <b>{}</b> — {} buys / {} sells\n",
        plain_usd(ov.vol_1h), ov.buys_1h, ov.sells_1h
    ));
    out.push_str(&format!(
        "└ 24h: <b>{}</b> — {} buys / {} sells\n\n",
        plain_usd(ov.vol_24h), ov.buys_24h, ov.sells_24h
    ));

    // Holders
    match holders {
        Some(h) => {
            out.push_str("<b>Holders</b>\n");
            out.push_str(&format!("├ Holders:    <b>{}</b>\n", h.holder_count));
            out.push_str(&format!("├ Top 10:     <b>{:.1}%</b>\n", h.top_10_pct));
            out.push_str(&format!("└ Top Holder: <b>{:.1}%</b>\n\n", h.top_holder_pct));
            if !h.top_holders.is_empty() {
                out.push_str("<b>Top Holders (excl. curve)</b>\n");
                out.push_str(&format_top_holders_tree(&h.top_holders, ov.market_cap_usd));
                out.push('\n');
            }
        }
        None if ov.chain == "solana" => {
            out.push_str("<b>Holders</b>\n└ <i>Unavailable right now (Helius RPC slow or not configured)</i>\n\n");
        }
        None => {
            out.push_str(&format!(
                "<b>Holders</b>\n└ <i>Not indexed for {} — check Bubblemaps below</i>\n\n",
                encode_text(&chain_uc)
            ));
        }
    }

    // Bot call performance (only if this token was actually alerted by the bot)
    if let Some(c) = call {
        if c.initial_market_cap_usd > 0.0 {
            let peak = c.highest_market_cap_usd.max(ov.market_cap_usd);
            let elapsed = (chrono::Utc::now() - c.alert_timestamp).num_seconds().max(0) as f64 / 60.0;
            out.push_str("<b>Bot Call</b>\n");
            out.push_str(&format!(
                "├ Called at: <b>{}</b> ({} ago)\n",
                plain_usd(c.initial_market_cap_usd),
                format_age_mins(elapsed)
            ));
            out.push_str(&format!(
                "├ Peak ATH:  <b>{:.2}x</b> ({})\n",
                peak / c.initial_market_cap_usd,
                plain_usd(peak)
            ));
            out.push_str(&format!(
                "└ Now:       <b>{:.2}x</b>\n\n",
                ov.market_cap_usd / c.initial_market_cap_usd
            ));
        }
    }

    // Socials
    let mut social_links: Vec<String> = Vec::new();
    for w in ov.websites.iter().filter(|u| u.starts_with("http")).take(2) {
        social_links.push(format!("<a href=\"{}\">Website</a>", html_escape::encode_double_quoted_attribute(w)));
    }
    for (kind, url) in ov.socials.iter().filter(|(_, u)| u.starts_with("http")) {
        let label = match kind.as_str() {
            "twitter" => "X".to_string(),
            "telegram" => "Telegram".to_string(),
            "discord" => "Discord".to_string(),
            other => encode_text(other).to_string(),
        };
        social_links.push(format!("<a href=\"{}\">{}</a>", html_escape::encode_double_quoted_attribute(url), label));
    }
    if !social_links.is_empty() {
        out.push_str(&format!("<b>Socials:</b> {}\n\n", social_links.join(" • ")));
    }

    // Links
    let dex_url = if ov.url.is_empty() {
        format!("https://dexscreener.com/{}/{}", ov.chain, addr)
    } else {
        ov.url.clone()
    };
    let bm_chain = match ov.chain.as_str() {
        "ethereum" => "eth",
        "bsc" => "bsc",
        "base" => "base",
        _ => "solana",
    };
    let mut links = vec![
        format!("<a href=\"{}\">DexScreener</a>", html_escape::encode_double_quoted_attribute(&dex_url)),
        format!("<a href=\"https://bubblemaps.io/{}/token/{}\">Bubblemaps</a>", bm_chain, addr),
    ];
    match ov.chain.as_str() {
        "solana" => {
            links.push(format!("<a href=\"https://axiom.trade/token/{}\">Axiom</a>", addr));
            links.push(format!("<a href=\"https://solscan.io/token/{}\">Solscan</a>", addr));
        }
        "ethereum" => links.push(format!("<a href=\"https://etherscan.io/token/{}\">Etherscan</a>", addr)),
        "base" => links.push(format!("<a href=\"https://basescan.org/token/{}\">Basescan</a>", addr)),
        "bsc" => links.push(format!("<a href=\"https://bscscan.com/token/{}\">BscScan</a>", addr)),
        _ => {}
    }
    out.push_str(&links.join(" • "));
    out.push_str(&format!("\n<i>Live data • {}</i>", chrono::Utc::now().format("%H:%M:%S UTC")));
    out
}
