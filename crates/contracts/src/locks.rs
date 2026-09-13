//! Pure lock-state rules. The sim calls install only after successful assignment checks.
use crate::*;
pub fn blocks(locks: &[OrderLock], tick: Tick, event_round: u32) -> bool {
    locks.iter().any(|lock| {
        lock.from_tick < tick && tick <= lock.until_tick && event_round < lock.issued_round
    })
}
pub fn canonicalize(locks: &mut Vec<OrderLock>, tick: Tick) -> Result<()> {
    if locks
        .iter()
        .any(|l| l.from_tick >= l.until_tick || l.issued_round == 0 || l.from_tick > tick)
    {
        return Err("invalid order lock interval/round or future installation".into());
    }
    locks.retain(|l| l.until_tick >= tick);
    locks.sort_by_key(|l| (l.until_tick, l.issued_round, l.from_tick));
    locks.dedup();
    let entries = locks.clone();
    locks.retain(|l| {
        !entries.iter().any(|other| {
            (
                other.until_tick,
                other.issued_round,
                std::cmp::Reverse(other.from_tick),
            ) > (l.until_tick, l.issued_round, std::cmp::Reverse(l.from_tick))
                && other.until_tick >= l.until_tick
                && other.issued_round >= l.issued_round
                && other.from_tick.max(tick.saturating_sub(1))
                    <= l.from_tick.max(tick.saturating_sub(1))
        })
    });
    Ok(())
}
pub fn install(
    locks: &mut Vec<OrderLock>,
    tick: Tick,
    round: u32,
    policy: FutureOrderPolicy,
    window: Option<Tick>,
    max_tick: Tick,
) -> Result<()> {
    if round == 0 {
        return Err("round zero is reserved for genesis".into());
    }
    let mut candidate = locks.clone();
    if let Some(interval) = crate::draft::lock_interval(tick, policy, window, max_tick)? {
        candidate.push(OrderLock {
            from_tick: tick,
            until_tick: interval.through_tick,
            issued_round: round,
        });
    }
    canonicalize(&mut candidate, tick)?;
    *locks = candidate;
    Ok(())
}
