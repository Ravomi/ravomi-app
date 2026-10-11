use std::{
    collections::BTreeSet,
    fs,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: cold_glyphs TEXT.txt FONT.ttf".into());
    }
    let text = fs::read_to_string(&args[0])?;
    let font = fs::read(&args[1])?;
    let face = ttf_parser::Face::parse(&font, 0)?;
    let characters: BTreeSet<_> = text.chars().filter(|c| !c.is_control()).collect();
    let missing: Vec<_> = characters
        .iter()
        .filter(|&&c| face.glyph_index(c).is_none())
        .collect();
    let glyphs: Vec<_> = characters
        .iter()
        .filter_map(|&c| face.glyph_index(c).map(|g| g.0))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    println!("{{\"unique_characters\":{},\"glyphs\":{},\"missing_characters\":{:?},\"texels_per_em\":{},\"pixel_range\":{}}}", characters.len(), glyphs.len(), missing.iter().map(|&&c| format!("U+{:04X}", c as u32)).collect::<Vec<_>>(), kayak_font_msdf::TEXELS_PER_EM, kayak_font_msdf::PIXEL_RANGE);
    let mut expected = None;
    for threads in [1, 4, 8] {
        for trial in 1..=3 {
            let next = AtomicUsize::new(0);
            let start = Instant::now();
            let results = std::thread::scope(|scope| {
                let workers: Vec<_> = (0..threads)
                    .map(|_| {
                        scope.spawn(|| -> Result<(u64, usize), String> {
                            let mut checksum = 0u64;
                            let mut bytes = 0;
                            loop {
                                let index = next.fetch_add(1, Ordering::Relaxed);
                                let Some(&id) = glyphs.get(index) else { break };
                                if let Some(bitmap) = kayak_font_msdf::generate(&font, 0, id)? {
                                    let mut hash = 0xcbf29ce484222325u64 ^ u64::from(id);
                                    for &byte in &bitmap.rgba {
                                        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
                                    }
                                    checksum = checksum.wrapping_add(hash);
                                    bytes += bitmap.rgba.len();
                                }
                            }
                            Ok((checksum, bytes))
                        })
                    })
                    .collect();
                workers
                    .into_iter()
                    .map(|w| w.join().map_err(|_| "glyph worker panicked".to_owned())?)
                    .collect::<Result<Vec<_>, String>>()
            })?;
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let result = results
                .iter()
                .fold((0u64, 0usize), |(sum, bytes), &(s, b)| {
                    (sum.wrapping_add(s), bytes + b)
                });
            if expected.is_some_and(|e| e != result) {
                return Err("glyph output differs between trials or worker counts".into());
            }
            expected = Some(result);
            println!("{{\"threads\":{threads},\"trial\":{trial},\"milliseconds\":{elapsed},\"rgba_bytes\":{},\"checksum\":\"{:016x}\"}}", result.1, result.0);
        }
    }
    Ok(())
}
