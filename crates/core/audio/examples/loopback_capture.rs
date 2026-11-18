#![allow(clippy::print_stdout)]

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    if std::env::var_os(mint_audio::loopback::LOOPBACK_VAR).is_none() {
        println!(
            "set {} to the WAV to write",
            mint_audio::loopback::LOOPBACK_VAR
        );
        return;
    }
    mint_audio::loopback::start_from_env();
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    let (stalls, lost_us) = mint_audio::loopback::stalls();
    #[allow(clippy::cast_precision_loss)]
    let lost_ms = lost_us as f64 / 1000.0;
    println!("{seconds} s: endpoint stalls {stalls}, {lost_ms:.1} ms lost");
}
