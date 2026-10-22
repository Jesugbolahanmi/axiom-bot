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
    let call_mc_k = call.initial_market_cap_usd / 1000.0;
    let ath_mc_k = call.highest_market_cap_usd / 1000.0;
    let curr_mc_k = call.current_market_cap_usd / 1000.0;
    let elapsed_mins = (chrono::Utc::now() - call.alert_timestamp).num_seconds() as f64 / 60.0;

    let short_mint = if call.mint.len() > 10 {
        format!("{}...{}", &call.mint[..4], &call.mint[call.mint.len() - 4..])
    } else {
        call.mint.clone()
    };

    let tag = if call.alert_type == AlertType::Revival { " [REVIVAL]" } else { "" };

    format!(
        "🎯 <b>CALL ATH UPDATE: ${}{} ({:.0}x Milestone Reached!)</b>\n\n        <b>Performance Metrics</b>\n        ├ Peak ATH:  <b>{:.2}x</b> (${:.1}K —&gt; <b>${:.1}K</b>)\n        ├ Current:   <b>{:.2}x</b> (${:.1}K)\n        ├ Mint:      <code>{}</code>\n        └ Alerted:   <b>{:.1}m ago</b>\n\n        <a href=\"https://dexscreener.com/solana/{}\">DexScreener</a> • <a href=\"https://bubblemaps.io/solana/token/{}\">Bubblemaps</a> • <a href=\"https://axiom.trade/token/{}\">Axiom</a>",
        encode_text(&call.symbol),
        tag,
        milestone,
        ath_multiple,
        call_mc_k,
        ath_mc_k,
        curr_multiple,
        curr_mc_k,
        short_mint,
        elapsed_mins,
        call.mint,
        call.mint,
        call.mint
    )
}


pub fn format_token_pnl_html(record: &CallRecord) -> String {
    let name = encode_text(&record.name);
    let sym = encode_text(&record.symbol);
    let mint = &record.mint;

    let init_mc = record.initial_market_cap_usd;
    let curr_mc = record.current_market_cap_usd;
    let highest_mc = record.highest_market_cap_usd;

    let curr_mult = if init_mc > 0.0 { curr_mc / init_mc } else { 1.0 };
    let peak_mult = if init_mc > 0.0 { highest_mc / init_mc } else { 1.0 };

    let curr_roi = (curr_mult - 1.0) * 100.0;
    let peak_roi = (peak_mult - 1.0) * 100.0;

    let sign_curr = if curr_roi >= 0.0 { "+" } else { "" };
    let sign_peak = if peak_roi >= 0.0 { "+" } else { "" };

    let sim_entry_sol = 0.10;
    let peak_val_sol = sim_entry_sol * peak_mult;
    let curr_val_sol = sim_entry_sol * curr_mult;

    let status_badge = if peak_mult >= 10.0 {
        "🚀 <b>[MOONSHOT 10X+]</b>"
    } else if peak_mult >= 5.0 {
        "💎 <b>[5X+ SURGE]</b>"
    } else if peak_mult >= 2.0 {
        "🎯 <b>[2X+ FLIP / WIN]</b>"
    } else if peak_mult >= 1.0 {
        "📈 <b>[IN PROFIT]</b>"
    } else {
        "🔻 <b>[UNDER CALL / LOSS]</b>"
    };

    let elapsed_mins = (chrono::Utc::now() - record.alert_timestamp).num_seconds() as f64 / 60.0;
    let elapsed_str = if elapsed_mins >= 1440.0 {
        format!("{:.1}d ago", elapsed_mins / 1440.0)
    } else if elapsed_mins >= 60.0 {
        format!("{:.1}h ago", elapsed_mins / 60.0)
    } else {
        format!("{:.1}m ago", elapsed_mins)
    };

    let tag = if record.alert_type == AlertType::Revival { " [REVIVAL]" } else { "" };

    format!(
        "📊 <b>${}{} ({}) — Token Performance &amp; ATH PnL</b>\n        <code>{}</code>\n\n        <b>Call Performance Metrics:</b>\n        ├ Status:       {}\n        ├ Call MCap:    <b>${:.1}K</b>\n        ├ Peak ATH:     <b>{:.2}x</b> (${:.1}K, {}{:.1}%)\n        ├ Current:      <b>{:.2}x</b> (${:.1}K, {}{:.1}%)\n        └ Alerted:      <b>{}</b>\n\n        <b>Simulated PnL (0.10 SOL Entry):</b>\n        ├ Initial:      0.10 SOL\n        ├ Peak Value:   <b>{:.3} SOL</b> ({}{:.1}%)\n        └ Current Val:  <b>{:.3} SOL</b> ({}{:.1}%)\n\n        <b>Live Liquidity &amp; Volume:</b>\n        ├ Liquidity:    <b>${:.1}K</b>\n        ├ 24h Volume:   <b>${:.1}K</b>\n        └ 5m Volume:    <b>${:.1}K</b>\n\n        <a href=\"https://axiom.trade/token/{}\">Axiom</a> • <a href=\"https://dexscreener.com/solana/{}\">DexScreener</a> • <a href=\"https://bubblemaps.io/solana/token/{}\">Bubblemaps</a> • <a href=\"https://solscan.io/token/{}\">Solscan</a>\n        <i>Last refreshed: {}</i>",
        sym,
        tag,
        name,
        mint,
        status_badge,
        init_mc / 1000.0,
        peak_mult,
        highest_mc / 1000.0,
        sign_peak,
        peak_roi,
        curr_mult,
        curr_mc / 1000.0,
        sign_curr,
        curr_roi,
        elapsed_str,
        peak_val_sol,
        sign_peak,
        peak_roi,
        curr_val_sol,
        sign_curr,
        curr_roi,
        record.current_liquidity_usd / 1000.0,
        record.current_volume_usd / 1000.0,
        record.current_volume_5m_usd / 1000.0,
        mint,
        mint,
        mint,
        mint,
        record.last_refreshed.format("%H:%M:%S UTC")
    )
}
