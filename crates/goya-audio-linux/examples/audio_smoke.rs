use goya_audio_linux::AudioOutput;

fn main() {
    let mut out = match goya_audio_linux::PwCatOutput::spawn() {
        Some(o) => o,
        None => {
            println!("no output");
            return;
        }
    };
    println!("live: {}", out.is_live());
    if out.is_live() {
        let tap = goya_audio_dsp::Tap::new(880.0, 48_000.0, 0.25);
        let mut limiter = goya_audio_dsp::Limiter::new();
        let mut buf = Vec::new();
        for i in 0..12_000 {
            let s = tap.sample(i as f32 / 48_000.0);
            buf.push(limiter.push(s * 0.6));
        }
        let n = goya_audio_linux::AudioOutput::write(&mut out, &buf).unwrap_or(0);
        println!(
            "wrote {n} samples, state={:?}, total={}",
            out.state(),
            out.samples_written()
        );
    }
}
