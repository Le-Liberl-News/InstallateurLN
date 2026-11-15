//! Dragon Slayer's floppy translator, carried over from its author's Python
//! script (dsretrack.py, in the patch archive) so that it runs anywhere with
//! nothing to install. It finds the game's texts through the code that points
//! at them, swaps them for the French ones (dragon_slayer_orig.json /
//! dragon_slayer_fr.json), packs them back into the freed spaces (subsets of
//! texts whose lengths fill each space), fixes the pointers and rewrites the
//! tracks. The behaviour, quirks included, is the script's: tests compare both
//! byte for byte on generated floppies.

use serde_json::Value;
use std::path::{Path, PathBuf};

struct Format {
    name: &'static str,
    chain: &'static [&'static [u8]],
    posi: Option<&'static [&'static [u8]]>,
    arg: &'static [&'static [u8]],
    program: std::ops::Range<usize>,
    data: &'static [usize],
    offset: i64,
    sectors: (usize, usize),
    inside: Option<&'static [&'static [u8]]>,
}

const FORMATS: &[Format] = &[
    Format {
        name: "Dragon Slayer, Level 1.1",
        chain: &[b"\xcd\x2c\x96"],
        posi: Some(&[b"\xcd\x09\x9a"]),
        arg: &[b"\x11"],
        program: 12..14,
        data: &[3],
        offset: 0x0,
        sectors: (0, 5),
        inside: Some(&[b"\xed\x53\xdf\xb9"]),
    },
    Format {
        name: "Dragon Slayer, Level 2.0",
        chain: &[b"\xcd\x0b\x13", b"\xc3\x0b\x13"],
        posi: None,
        arg: &[b"\xdd\x21", b"\xfd\x21"],
        program: 3..7,
        data: &[8, 11],
        offset: 0x5000,
        sectors: (1, 5),
        inside: None,
    },
    Format {
        name: "Dragon Slayer, LOGiN version",
        chain: &[b"\xcd\x79\x0d"],
        posi: None,
        arg: &[b"\xdd\x21", b"\xfd\x21", b"\x32", b"\x3a"],
        program: 3..4,
        data: &[5, 6],
        offset: 0x2000,
        sectors: (0, 5),
        inside: Some(&[b"\xcd\x21\x0e"]),
    },
];

fn special(byte: u8) -> Option<&'static str> {
    Some(match byte {
        0x24 => "$",
        0x25 => "⚱",
        0x26 => "♕",
        0x27 => "🧍",
        0x28 => "♥",
        0x29 => "⛏",
        0x2a => "🧠",
        0x2b => "×",
        0x2c => "!",
        0x30 => "・",
        0x31 => ".",
        0x32 => "-",
        0x33 => "↵",
        0x34 => "↑",
        0x35 => "↓",
        0x36 => "←",
        0x37 => "→",
        0x38 => "␛",
        0x39 => "⇧",
        0x3a => "␠",
        0x3b => "↖",
        0x3c => "↗",
        0x3d => "↘",
        0x3e => "↙",
        0x40 => "┌",
        0x41 => "┐",
        0x42 => "└",
        0x43 => "┘",
        0x44 => "─",
        0x45 => "│",
        0x46 => "┼",
        0x47 => "├",
        0x48 => "┤",
        0x49 => "┬",
        0x4a => "┴",
        0x4b => "[",
        0x4c => "]",
        0x9f | 0x3f => " ",
        _ => return None,
    })
}

/// The byte of a special character (the script's reversed table: " " is 0x3f, its last entry).
fn back(text: &str) -> Option<u8> {
    if text == " " {
        return Some(0x3f);
    }
    (0x24u8..=0x4c).find(|byte| special(*byte) == Some(text))
}

/// A text the code points at: a chain of positioned lines, or a single movable line.
#[derive(Clone, PartialEq, Debug)]
enum Text {
    Chain(Vec<(i64, String)>),
    Movable(String),
}

type Tracks = Vec<Option<Vec<Vec<u8>>>>;

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > haystack.len() {
        return None;
    }
    haystack[from..].windows(needle.len()).position(|window| window == needle).map(|at| at + from)
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|window| window == needle)
}

/// Python's int.from_bytes(little) of a slice (0 to 2 bytes here).
fn little(bytes: &[u8]) -> i64 {
    bytes.iter().rev().fold(0i64, |value, byte| (value << 8) | *byte as i64)
}

/// Python's sequence[start:end] with negative bounds.
fn slice(bytes: &[u8], start: i64, end: i64) -> &[u8] {
    let length = bytes.len() as i64;
    let fix = |index: i64| if index < 0 { (index + length).max(0) } else { index.min(length) } as usize;
    let (a, b) = (fix(start), fix(end));
    if a >= b { &[] } else { &bytes[a..b] }
}

fn read_string(text: &[u8]) -> Option<String> {
    let text = match text.iter().position(|byte| *byte == 0xff) {
        Some(at) => &text[..at],
        None => text,
    };
    let mut out = String::new();
    let mut in_tag = false;
    for &byte in text {
        if byte == 0xfe {
            out.push_str("<color:");
            in_tag = true;
        } else if byte < 10 || in_tag {
            out.push_str(&byte.to_string());
            if in_tag {
                out.push('>');
                in_tag = false;
            }
        } else if byte < 36 {
            out.push((byte + 0x37) as char);
        } else if let Some(symbol) = special(byte) {
            out.push_str(symbol);
        } else {
            return None;
        }
    }
    Some(out)
}

/// None: the script would stop on an error there.
fn read_chain(start: usize, binary: &[u8]) -> Result<Option<(Text, (i64, i64))>, String> {
    let strings = &binary[start.min(binary.len())..];
    let end = find(strings, b"\xff\xff", 0).ok_or("chaîne sans fin")?;
    let strings = &strings[..end];
    let length = (start as i64, start as i64 + strings.len() as i64 + 2);
    let mut decoded = Vec::new();
    for text in strings.split(|byte| *byte == 0xff) {
        if text.is_empty() {
            continue;
        }
        let coords = little(&text[..text.len().min(2)]);
        match read_string(&text[text.len().min(2)..]) {
            Some(line) if !line.is_empty() => decoded.push((coords, line)),
            _ => return Ok(None),
        }
    }
    Ok(Some((Text::Chain(decoded), length)))
}

fn read_movable(start: usize, binary: &[u8]) -> Result<Option<(Text, (i64, i64))>, String> {
    let text = &binary[start.min(binary.len())..];
    let end = text.iter().position(|byte| *byte == 0xff).ok_or("texte sans fin")?;
    let text = &text[..end];
    Ok(read_string(text).map(|line| (Text::Movable(line), (start as i64, start as i64 + text.len() as i64 + 1))))
}

fn empty(text: &Text) -> bool {
    match text {
        Text::Chain(lines) => lines.is_empty(),
        Text::Movable(line) => line.is_empty(),
    }
}

type Reader = fn(usize, &[u8]) -> Result<Option<(Text, (i64, i64))>, String>;

/// The texts the code points at through `signatures`: their spaces and the pointers to each.
#[allow(clippy::type_complexity)]
fn get_txt(code: &[u8], data: &[u8], read: Reader, form: &Format, signatures: &[&[u8]]) -> Result<(Vec<Text>, Vec<(i64, i64)>, Vec<Vec<usize>>), String> {
    let (mut txt, mut freedom, mut references): (Vec<Text>, Vec<(i64, i64)>, Vec<Vec<usize>>) = (Vec::new(), Vec::new(), Vec::new());
    for signature in signatures {
        let mut step = 0usize;
        while let Some(offset) = find(code, signature, step) {
            let cut = slice(code, offset as i64 - 0x20, offset as i64);
            let mut args = Vec::new();
            for arg in form.arg {
                if let Some(at) = rfind(cut, arg) {
                    args.push(at as i64 - 0x20 + arg.len() as i64);
                }
            }
            for arg in args {
                let reference = offset as i64 + arg;
                let address = little(slice(code, reference, reference + 2)) - form.offset;
                if address < 0 || (data.len() as i64) < address {
                    continue;
                }
                let Some((text, space)) = read(address as usize, data)? else { continue };
                if empty(&text) {
                    continue;
                }
                // A negative reference would index from the end in the script; it cannot come from a signature found here.
                let reference = reference.max(0) as usize;
                match txt.iter().position(|known| *known == text) {
                    Some(index) => references[index].push(reference),
                    None => {
                        txt.push(text);
                        freedom.push(space);
                        references.push(vec![reference]);
                    }
                }
            }
            step = offset + 1;
        }
    }
    Ok((txt, freedom, references))
}

fn to_bytes(text: &str) -> Result<Vec<u8>, String> {
    let characters: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let rest: String = characters[index..].iter().collect();
        let character = characters[index];
        if characters.len() - index > 7 && rest.starts_with("<color:") {
            let close = characters[index..].iter().position(|c| *c == '>').ok_or("balise de couleur sans fin")? + index;
            let number: String = characters[index + 7..close].iter().collect();
            let value: u8 = number.trim().parse().map_err(|_| format!("couleur invalide : {number}"))?;
            out.push(0xfe);
            out.push(value);
            index = close;
        } else if let Some(byte) = back(&character.to_string()) {
            out.push(byte);
        } else if character.is_ascii_alphanumeric() {
            out.push(character.to_digit(36).unwrap() as u8);
        } else {
            return Err(format!("caractère impossible à écrire : « {character} »"));
        }
        index += 1;
    }
    out.push(0xff);
    Ok(out)
}

fn chain_to_bytes(chain: &[(i64, String)]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for (coords, line) in chain {
        let coords = u16::try_from(*coords).map_err(|_| "coordonnées invalides")?;
        out.extend(coords.to_le_bytes());
        out.extend(to_bytes(line)?);
    }
    out.extend(b"\xff\xff");
    Ok(out)
}

fn text_of(value: &Value) -> Option<Text> {
    match value {
        Value::String(line) => Some(Text::Movable(line.clone())),
        Value::Array(lines) => lines
            .iter()
            .map(|line| Some((line.get(0)?.as_i64()?, line.get(1)?.as_str()?.to_string())))
            .collect::<Option<Vec<_>>>()
            .map(Text::Chain),
        _ => None,
    }
}

fn txt_to_bytes(txt: Vec<Text>, original: &[Text], french: &[Text]) -> Result<Vec<Vec<u8>>, String> {
    let mut known = false;
    let mut out = Vec::new();
    for text in txt {
        let text = match original.iter().position(|candidate| *candidate == text) {
            Some(index) => {
                known = true;
                french.get(index).cloned().ok_or("traduction manquante")?
            }
            None => text,
        };
        out.push(match &text {
            Text::Chain(lines) => chain_to_bytes(lines)?,
            Text::Movable(line) => to_bytes(line)?,
        });
    }
    if !known {
        return Err("textes non reconnus".into());
    }
    Ok(out)
}

/// The first subset of `lengths` (increasing indices) adding up to `target`:
/// the same dynamic programming as the subsetsum.py shipped with the patch.
fn solution(lengths: &[usize], target: usize) -> Option<Vec<usize>> {
    if target == 0 {
        return None;
    }
    let mut reach: std::collections::BTreeMap<usize, Option<usize>> = std::collections::BTreeMap::new();
    reach.insert(0, None);
    for (index, &length) in lengths.iter().enumerate() {
        if length == 0 {
            continue;
        }
        let sums: Vec<usize> = reach.keys().rev().copied().collect();
        for total in sums {
            let new = total + length;
            if new <= target && !reach.contains_key(&new) {
                reach.insert(new, Some(index));
            }
        }
        if reach.contains_key(&target) {
            break;
        }
    }
    if !reach.contains_key(&target) {
        return None;
    }
    let mut chosen = Vec::new();
    let mut total = target;
    while total != 0 {
        let index = reach[&total].expect("somme atteinte par un texte");
        chosen.push(index);
        total -= lengths[index];
    }
    chosen.sort_unstable();
    Some(chosen)
}

fn modify_code(code: &mut [u8], reference: usize, step: i64) -> Result<(), String> {
    let value = u16::try_from(step).map_err(|_| "adresse hors de portée")?;
    if reference + 2 > code.len() {
        return Err("pointeur hors du programme".into());
    }
    code[reference..reference + 2].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

enum Inside {
    Offset(i64),
    /// Not inside any text: the script would fail if it were ever used.
    Outside,
}

fn change(mut code: Vec<u8>, mut data: Vec<u8>, form: &Format, original: &[Text], french: &[Text]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (mut txt, mut freedom, mut references) = get_txt(&code, &data, read_chain, form, form.chain)?;
    if let Some(posi) = form.posi {
        let (a, b, c) = get_txt(&code, &data, read_movable, form, posi)?;
        txt.extend(a);
        freedom.extend(b);
        references.extend(c);
    }
    let mut inside_ids: Vec<usize> = Vec::new();
    let mut inside_space: Vec<Inside> = Vec::new();
    let mut inside_refs: Vec<Vec<usize>> = Vec::new();
    if let Some(inside) = form.inside {
        let (_, spaces, refs) = get_txt(&code, &data, read_movable, form, inside)?;
        inside_refs = refs;
        for space in spaces {
            match freedom.iter().position(|free| space.0 > free.0 && space.0 < free.1) {
                Some(j) => {
                    inside_ids.push(j);
                    inside_space.push(Inside::Offset(space.0 - freedom[j].0));
                }
                None => inside_space.push(Inside::Outside),
            }
        }
    }
    freedom.sort();
    let count = freedom.len();
    for i in 1..count {
        while i < freedom.len() && freedom[i].0 - freedom[i - 1].1 < 4 {
            let merged = freedom.remove(i);
            freedom[i - 1] = (freedom[i - 1].0, merged.1);
        }
    }

    let mut txt = txt_to_bytes(txt, original, french)?;
    let mut lengths: Vec<usize> = txt.iter().map(Vec::len).collect();
    for space in freedom {
        let mut step = space.0;
        let mut target = space.1 - step;
        let mut result = None;
        while result.is_none() && target != 0 {
            result = if target > 0 { solution(&lengths, target as usize) } else { None };
            target -= 1;
            if target < -(1 << 20) {
                break;
            }
        }
        let result = result.ok_or("pas assez d'espace libre pour inclure les traductions")?;
        for &i in result.iter().rev() {
            let current = txt.remove(i);
            let current_step = step;
            let reference = references.remove(i);
            let plus = step + current.len() as i64;
            if plus <= space.1 {
                let at = step as usize;
                data.splice(at..at + current.len(), current.iter().copied());
                for r in reference {
                    modify_code(&mut code, r, step + form.offset)?;
                }
                step = plus;
            }
            if form.inside.is_some() {
                if let Some(position) = inside_ids.iter().position(|id| *id == i) {
                    inside_ids.remove(position);
                    let space = inside_space.remove(position);
                    let refs = inside_refs.remove(position);
                    let Inside::Offset(offset) = space else { return Err("texte intérieur hors de tout espace".into()) };
                    for r in refs {
                        modify_code(&mut code, r, current_step + form.offset + offset)?;
                    }
                }
                inside_ids = inside_ids.into_iter().map(|present| present - usize::from(present > i)).collect();
            }
            lengths.remove(i);
        }
    }
    if !txt.is_empty() {
        return Err("pas assez d'espace libre pour inclure les traductions".into());
    }
    Ok((code, data))
}

fn track_offsets(file: &[u8]) -> Vec<usize> {
    (0x20..0x160).step_by(4).map(|at| file.get(at..at + 4).map(little4).unwrap_or(0)).collect()
}

fn little4(bytes: &[u8]) -> usize {
    u32::from_le_bytes(bytes.try_into().unwrap_or([0; 4])) as usize
}

fn detrack(file: &[u8]) -> Result<Tracks, String> {
    let addresses = track_offsets(file);
    let mut tracks: Tracks = vec![None; addresses.len()];
    for address in addresses {
        let mut current: Option<usize> = None;
        let mut position = address;
        loop {
            let header = file.get(position..(position + 0x10).min(file.len())).unwrap_or(&[]);
            if header.is_empty() {
                break;
            }
            if header.len() < 4 {
                return Err("en-tête de secteur tronqué".into());
            }
            position += header.len();
            let (cylinder, side, size) = (header[0] as usize, header[1] as usize, 128usize << header[3]);
            let track = 2 * cylinder + side;
            if current.is_none() {
                if track > tracks.len() {
                    break;
                }
                if track == tracks.len() {
                    return Err("piste invalide".into());
                }
                current = Some(track);
                tracks[track] = Some(Vec::new());
            }
            if current != Some(track) {
                break;
            }
            let end = (position + size).min(file.len());
            let sector = file.get(position..end).unwrap_or(&[]).to_vec();
            position = end;
            tracks[track].as_mut().unwrap().push(sector);
        }
    }
    Ok(tracks)
}

fn parse_content(tracks: &Tracks, form: &Format, list: &[usize]) -> Option<Vec<u8>> {
    let (start, end) = form.sectors;
    let mut out = Vec::new();
    for &track in list {
        let sectors = tracks.get(track)?.as_ref()?;
        for sector in sectors.iter().take(end).skip(start) {
            out.extend_from_slice(sector);
        }
    }
    Some(out)
}

fn retrack(tracks: &mut Tracks, mut data: &[u8], form: &Format, list: &[usize]) -> Result<(), String> {
    let (start, end) = form.sectors;
    for &track in list {
        for sector in start..end {
            let slot = tracks.get_mut(track).and_then(Option::as_mut).and_then(|sectors| sectors.get_mut(sector)).ok_or("secteur absent")?;
            let take = slot.len().min(data.len());
            *slot = data[..take].to_vec();
            data = &data[take..];
        }
    }
    Ok(())
}

fn rewrite(original: &[u8], tracks: &Tracks) -> Result<Vec<u8>, String> {
    let addresses = track_offsets(original);
    if addresses.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err("pistes désordonnées".into());
    }
    let mut out = Vec::new();
    let mut position = 0usize;
    let mut step = 0usize;
    let read = |position: &mut usize, count: i64| -> Vec<u8> {
        let end = if count < 0 { original.len() } else { (*position + count as usize).min(original.len()) };
        let chunk = original.get(*position..end).unwrap_or(&[]).to_vec();
        *position = end.max(*position);
        chunk
    };
    for (i, track) in tracks.iter().enumerate() {
        let Some(sectors) = track.as_ref().filter(|sectors| !sectors.is_empty()) else { continue };
        out.extend(read(&mut position, addresses[i] as i64 - step as i64));
        step = addresses[i];
        for sector in sectors {
            out.extend(read(&mut position, 0x10));
            out.extend_from_slice(sector);
            step += 0x10 + sector.len();
            position = step;
        }
    }
    Ok(out)
}

/// Translates one floppy; the French copy is written next to it ("<name> [FR].d88").
pub fn translate(disk: &Path, patch: &Path) -> Result<PathBuf, String> {
    let read_json = |name: &str| -> Result<Vec<Text>, String> {
        let text = std::fs::read_to_string(patch.join(name)).map_err(|error| format!("{name} : {error}"))?;
        let value: Value = serde_json::from_str(&text).map_err(|error| format!("{name} : {error}"))?;
        value.as_array().ok_or(format!("{name} : liste attendue"))?.iter().map(|item| text_of(item).ok_or(format!("{name} : texte illisible"))).collect()
    };
    let original = read_json("dragon_slayer_orig.json")?;
    let french = read_json("dragon_slayer_fr.json")?;
    let file = std::fs::read(disk).map_err(|error| error.to_string())?;
    let target = output_name(disk);
    let bytes = run(&file, &original, &french)?;
    std::fs::write(&target, bytes).map_err(|error| format!("{} : {error}", target.display()))?;
    Ok(target)
}

/// The version of Dragon Slayer on a floppy, None when it is not one this patch knows.
pub fn recognize(file: &[u8]) -> Option<&'static str> {
    let tracks = detrack(file).ok()?;
    FORMATS
        .iter()
        .find(|form| parse_content(&tracks, form, &form.program.clone().collect::<Vec<_>>()).is_some_and(|code| find(&code, form.chain[0], 0).is_some()))
        .map(|form| form.name)
}

/// The French floppy's bytes, or why there is none.
fn run(file: &[u8], original: &[Text], french: &[Text]) -> Result<Vec<u8>, String> {
    let mut tracks = detrack(file)?;
    let (form, code) = FORMATS
        .iter()
        .find_map(|form| {
            let code = parse_content(&tracks, form, &form.program.clone().collect::<Vec<_>>())?;
            find(&code, form.chain[0], 0).map(|_| (form, code))
        })
        .ok_or("version de Dragon Slayer non reconnue")?;
    let _ = form.name;
    let data = parse_content(&tracks, form, form.data).ok_or("pistes de données absentes")?;
    let (code, data) = change(code, data, form, original, french)?;
    retrack(&mut tracks, &data, form, form.data)?;
    retrack(&mut tracks, &code, form, &form.program.clone().collect::<Vec<_>>())?;
    rewrite(file, &tracks)
}

/// "<name minus its last 4 characters> [FR].d88", as the script names it.
fn output_name(disk: &Path) -> PathBuf {
    let name = disk.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    let characters: Vec<char> = name.chars().collect();
    let stem: String = characters[..characters.len().saturating_sub(4)].iter().collect();
    disk.with_file_name(format!("{stem} [FR].d88"))
}

#[cfg(test)]
mod tests {
    /// Floppies generated by tests/dsretrack/compare.py and translated by the Python
    /// script are translated here too: the bytes must be the same. (cargo test -- --ignored)
    #[test]
    #[ignore]
    fn same_bytes_as_the_python_script() {
        let dir = std::path::PathBuf::from(std::env::var("DSRETRACK_CASES").expect("DSRETRACK_CASES"));
        let patch = dir.join("patcher");
        let read = |name: &str| -> Vec<super::Text> {
            let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(patch.join(name)).unwrap()).unwrap();
            value.as_array().unwrap().iter().map(|item| super::text_of(item).unwrap()).collect()
        };
        let (original, french) = (read("dragon_slayer_orig.json"), read("dragon_slayer_fr.json"));
        let mut compared = 0;
        for entry in std::fs::read_dir(dir.join("cases")).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("d88") || path.to_string_lossy().contains("[FR]") {
                continue;
            }
            let expected = std::fs::read(super::output_name(&path)).ok();
            let ours = super::run(&std::fs::read(&path).unwrap(), &original, &french).ok();
            assert_eq!(ours, expected, "{}", path.display());
            compared += 1;
        }
        assert!(compared > 0);
        println!("{compared} disquettes identiques");
    }
}
