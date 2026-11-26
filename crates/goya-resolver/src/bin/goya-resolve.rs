//! `goya-resolve` runs an admitted `.gscene` without a platform compiler.
//!
//! `--watch` polls the scene file and calls `Instance::reload`. `--suspend`
//! and `--resume` change the in-memory phase. `--trace` prints the display
//! sites a dispatch evaluated and the layout generation. It is not a timer.
//! `--benchmark` is named by GRD-001 and is not implemented. It exits with
//! status 2.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

/// How often `--watch` reads the file. This interval is not a measured budget.
const WATCH_POLL: Duration = Duration::from_millis(50);

use goya_lang::InteractionValue;
use goya_resolver::{
    Event, PerformanceProfile, Resolver, ResourcePolicy, SemanticContract, Target,
};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("goya-resolve: {message}");
            ExitCode::from(2)
        }
    }
}

enum Op {
    Set(String, String),
    Dispatch(String),
    Point(i64, i64),
    Enter(String),
    Scroll(i64, i64, i64, i64),
    Insert(String),
    Backspace,
    Delete,
    Caret(goya_resolver::CaretMotion),
    Suspend,
    Resume,
}

fn run(args: Vec<String>) -> Result<ExitCode, String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(flag) = args
        .iter()
        .find(|arg| matches!(arg.as_str(), "--benchmark"))
    {
        return Err(format!("{flag} is not implemented in GY-RESOLVER/0.1"));
    }

    let mut path = None;
    let mut viewport = None;
    let mut contract = None;
    let mut performance = PerformanceProfile::Standard;
    let mut target = None;
    let mut present = false;
    let mut watch = false;
    let mut trace = false;
    let mut validate_only = false;
    let mut ops = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--validate-only" => validate_only = true,
            "--present" => present = true,
            "--watch" => watch = true,
            "--trace" => trace = true,
            "--inspect" => {}
            "--viewport" => {
                index += 1;
                viewport = Some(parse_viewport(required(
                    &args,
                    index,
                    "--viewport needs WxH",
                )?)?);
            }
            "--contract" => {
                index += 1;
                let value = required(&args, index, "--contract needs a GSSH name")?;
                contract = Some(
                    SemanticContract::parse(value)
                        .ok_or_else(|| format!("unknown contract {value}"))?,
                );
            }
            "--target" => {
                index += 1;
                let value = required(
                    &args,
                    index,
                    "--target needs android, android_tv, browser, ios, linux, or windows",
                )?;
                target =
                    Some(Target::parse(value).ok_or_else(|| format!("unknown platform {value}"))?);
            }
            "--profile" => {
                index += 1;
                let value = required(
                    &args,
                    index,
                    "--profile needs constrained, standard, or performance",
                )?;
                performance = PerformanceProfile::parse(value)
                    .ok_or_else(|| format!("unknown performance profile {value}"))?;
            }
            "--pointer" => {
                index += 1;
                let value = required(&args, index, "--pointer needs x,y millipoints")?;
                let (x_mp, y_mp) = parse_point(value)?;
                ops.push(Op::Point(x_mp, y_mp));
            }
            "--enter" => {
                index += 1;
                ops.push(Op::Enter(
                    required(&args, index, "--enter needs text")?.to_owned(),
                ));
            }
            "--scroll" => {
                index += 1;
                let value = required(&args, index, "--scroll needs x,y,dx,dy millipoints")?;
                let (x_mp, y_mp, dx_mp, dy_mp) = parse_scroll(value)?;
                ops.push(Op::Scroll(x_mp, y_mp, dx_mp, dy_mp));
            }
            "--insert" => {
                index += 1;
                ops.push(Op::Insert(
                    required(&args, index, "--insert needs text")?.to_owned(),
                ));
            }
            "--backspace" => ops.push(Op::Backspace),
            "--delete" => ops.push(Op::Delete),
            "--caret" => {
                index += 1;
                let value = required(&args, index, "--caret needs left, right, home, or end")?;
                ops.push(Op::Caret(parse_caret(value)?));
            }
            "--suspend" => ops.push(Op::Suspend),
            "--resume" => ops.push(Op::Resume),
            "--dispatch" => {
                index += 1;
                ops.push(Op::Dispatch(
                    required(&args, index, "--dispatch needs an action")?.to_owned(),
                ));
            }
            "--set" => {
                index += 1;
                let value = required(&args, index, "--set needs element=text")?;
                let (element, text) = value.split_once('=').ok_or("--set needs element=text")?;
                ops.push(Op::Set(element.to_owned(), text.to_owned()));
            }
            other if other.starts_with('-') => return Err(format!("unknown argument {other}")),
            other if path.is_some() => return Err(format!("unexpected argument {other}")),
            other => path = Some(PathBuf::from(other)),
        }
        index += 1;
    }
    if validate_only && ops.iter().any(|op| matches!(op, Op::Suspend | Op::Resume)) {
        return Err("--validate-only does not suspend or resume".to_owned());
    }
    if validate_only && !ops.is_empty() {
        return Err("--validate-only does not dispatch or set fields".to_owned());
    }
    if validate_only && present {
        return Err("--validate-only does not present".to_owned());
    }
    if validate_only && watch {
        return Err("--validate-only does not watch".to_owned());
    }
    if present && target.is_none() {
        return Err("--present needs --target".to_owned());
    }
    if ops.iter().any(|op| matches!(op, Op::Point(_, _))) && target.is_none() {
        return Err("--pointer needs --target".to_owned());
    }
    if ops.iter().any(|op| matches!(op, Op::Enter(_))) && target.is_none() {
        return Err("--enter needs --target".to_owned());
    }
    if ops.iter().any(|op| matches!(op, Op::Scroll(_, _, _, _))) && target.is_none() {
        return Err("--scroll needs --target".to_owned());
    }
    if ops.iter().any(|op| matches!(op, Op::Insert(_))) && target.is_none() {
        return Err("--insert needs --target".to_owned());
    }
    if ops
        .iter()
        .any(|op| matches!(op, Op::Backspace | Op::Delete))
        && target.is_none()
    {
        return Err("--backspace and --delete need --target".to_owned());
    }
    if ops.iter().any(|op| matches!(op, Op::Caret(_))) && target.is_none() {
        return Err("--caret needs --target".to_owned());
    }
    let path = path.ok_or("missing .gscene path")?;
    let policy = ResourcePolicy {
        performance,
        ..ResourcePolicy::default()
    };
    let mut builder = Resolver::builder().resource_policy(policy);
    if let Some((width, height)) = viewport {
        builder = builder.viewport(width, height);
    }
    if let Some(contract) = contract {
        builder = builder.contract(contract);
    }
    if let Some(target) = target {
        builder = builder.target(target);
    }
    let resolver = match builder.build() {
        Ok(resolver) => resolver,
        Err(error) => {
            eprintln!("{error}");
            return Ok(ExitCode::from(1));
        }
    };
    let application = match resolver.load_path(&path) {
        Ok(application) => application,
        Err(error) => {
            eprintln!("{error}");
            return Ok(ExitCode::from(1));
        }
    };
    // Snapshot before any stdout, so a rewrite that arrives after the summary
    // is still a change against the bytes that were admitted.
    let watched = if watch {
        match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                eprintln!("goya-resolve: {}: {error}", path.display());
                return Ok(ExitCode::from(1));
            }
        }
    } else {
        None
    };
    if validate_only {
        println!("contract: {}", application.semantic_contract());
        print_target(application.target());
        println!("digest: {}", hex(&application.source_digest()));
        println!("actions: {}", application.actions().len());
        println!("ir: {}", ir_label(application.ir_lowered()));
        match application.execution_refusal() {
            Some(reason) => println!("execution: refused: {reason}"),
            None => println!("execution: ready"),
        }
        println!("performance: {} recorded", performance.as_str());
        if trace {
            let execution = if application.execution_refusal().is_some() {
                "refused"
            } else {
                "ready"
            };
            println!(
                "trace validate actions {} execution {execution}",
                application.actions().len()
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    let mut instance = match resolver.instantiate(&application) {
        Ok(instance) => instance,
        Err(error) => {
            eprintln!("{error}");
            return Ok(ExitCode::from(1));
        }
    };
    if let Err(error) = instance.start() {
        eprintln!("{error}");
        return Ok(ExitCode::from(1));
    }
    for op in &ops {
        match op {
            Op::Set(element, text) => {
                if let Err(error) = instance.set_field(element, text) {
                    eprintln!("{error}");
                    return Ok(ExitCode::from(1));
                }
            }
            Op::Point(x_mp, y_mp) => match instance.point(*x_mp, *y_mp) {
                Ok(goya_resolver::PointOutcome::Miss) => {
                    println!("point miss");
                    trace_layout(trace, "point miss", instance.layout_generations());
                }
                Ok(goya_resolver::PointOutcome::Focus { id, .. }) => {
                    println!("point focus {}", id.as_deref().unwrap_or("-"));
                    trace_layout(
                        trace,
                        &format!("point focus {}", id.as_deref().unwrap_or("-")),
                        instance.layout_generations(),
                    );
                }
                Ok(goya_resolver::PointOutcome::Activate { name, outcome }) => {
                    let affected = if outcome.affected_elements.is_empty() {
                        "-".to_owned()
                    } else {
                        outcome.affected_elements.join(",")
                    };
                    println!(
                        "point activate {name} revision {} affected {affected}",
                        outcome.revision
                    );
                    if let Some(announcement) = outcome.announcement {
                        println!("announcement: {announcement}");
                    }
                    trace_sites(
                        trace,
                        "point",
                        &name,
                        outcome.sites_evaluated,
                        instance.layout_generations(),
                    );
                }
                Err(error) => {
                    eprintln!("{error}");
                    return Ok(ExitCode::from(1));
                }
            },
            Op::Enter(text) => match instance.enter(text) {
                Ok(id) => println!("enter {id}"),
                Err(error) => {
                    eprintln!("{error}");
                    return Ok(ExitCode::from(1));
                }
            },
            Op::Scroll(x_mp, y_mp, dx_mp, dy_mp) => {
                match instance.scroll(*x_mp, *y_mp, *dx_mp, *dy_mp) {
                    Ok(goya_resolver::ScrollOutcome::Miss) => println!("scroll miss"),
                    Ok(goya_resolver::ScrollOutcome::Moved { name, x_mp, y_mp }) => {
                        println!("scroll {name} {x_mp},{y_mp}");
                    }
                    Err(error) => {
                        eprintln!("{error}");
                        return Ok(ExitCode::from(1));
                    }
                }
            }
            Op::Insert(text) => match instance.edit(goya_resolver::Edit::Insert(text.clone())) {
                Ok(place) => println!("insert {} {}", place.id, place.byte),
                Err(error) => {
                    eprintln!("{error}");
                    return Ok(ExitCode::from(1));
                }
            },
            Op::Backspace => match instance.edit(goya_resolver::Edit::Backspace) {
                Ok(place) => println!("backspace {} {}", place.id, place.byte),
                Err(error) => {
                    eprintln!("{error}");
                    return Ok(ExitCode::from(1));
                }
            },
            Op::Delete => match instance.edit(goya_resolver::Edit::Delete) {
                Ok(place) => println!("delete {} {}", place.id, place.byte),
                Err(error) => {
                    eprintln!("{error}");
                    return Ok(ExitCode::from(1));
                }
            },
            Op::Caret(motion) => match instance.edit(goya_resolver::Edit::Move(*motion)) {
                Ok(place) => println!("caret {} {}", place.id, place.byte),
                Err(error) => {
                    eprintln!("{error}");
                    return Ok(ExitCode::from(1));
                }
            },
            Op::Suspend => match instance.suspend() {
                Ok(()) => {
                    println!("suspend {}", instance.phase().as_str());
                    trace_layout(trace, "suspend", instance.layout_generations());
                }
                Err(error) => {
                    eprintln!("{error}");
                    return Ok(ExitCode::from(1));
                }
            },
            Op::Resume => match instance.resume() {
                Ok(()) => {
                    println!("resume {}", instance.phase().as_str());
                    trace_layout(trace, "resume", instance.layout_generations());
                }
                Err(error) => {
                    eprintln!("{error}");
                    return Ok(ExitCode::from(1));
                }
            },
            Op::Dispatch(action) => {
                match instance.dispatch(&Event::Activate {
                    action: action.clone(),
                }) {
                    Ok(outcome) => {
                        let affected = if outcome.affected_elements.is_empty() {
                            "-".to_owned()
                        } else {
                            outcome.affected_elements.join(",")
                        };
                        println!(
                            "dispatch {action} revision {} affected {affected}",
                            outcome.revision
                        );
                        if let Some(announcement) = outcome.announcement {
                            println!("announcement: {announcement}");
                        }
                        trace_sites(
                            trace,
                            "dispatch",
                            action,
                            outcome.sites_evaluated,
                            instance.layout_generations(),
                        );
                    }
                    Err(error) => {
                        eprintln!("{error}");
                        return Ok(ExitCode::from(1));
                    }
                }
            }
        }
    }
    print_instance(&instance, performance);
    if present {
        let target = target.expect("present checks --target");
        match print_projection(&mut instance, target) {
            Ok(nodes) => trace_present(trace, nodes, instance.layout_generations()),
            Err(error) => {
                eprintln!("{error}");
                return Ok(ExitCode::from(1));
            }
        }
    }
    if let Some(watched) = watched {
        let _ = io::stdout().flush();
        return watch_source(&path, watched, &mut instance, present, target, trace);
    }
    if let Err(error) = instance.terminate() {
        eprintln!("{error}");
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

fn watch_source(
    path: &Path,
    mut announced: Vec<u8>,
    instance: &mut goya_resolver::Instance,
    present: bool,
    target: Option<Target>,
    trace: bool,
) -> Result<ExitCode, String> {
    let mut last_io: Option<String> = None;
    loop {
        thread::sleep(WATCH_POLL);
        match fs::read(path) {
            Err(error) => {
                let message = format!("{}: {error}", path.display());
                if last_io.as_deref() != Some(message.as_str()) {
                    eprintln!("goya-resolve: {message}");
                    let _ = io::stderr().flush();
                    last_io = Some(message);
                }
            }
            Ok(bytes) => {
                last_io = None;
                if bytes == announced {
                    continue;
                }
                let source = match String::from_utf8(bytes.clone()) {
                    Ok(source) => source,
                    Err(_) => {
                        announced = bytes;
                        eprintln!("goya-resolve: {}: replacement is not UTF-8", path.display());
                        let _ = io::stderr().flush();
                        continue;
                    }
                };
                match instance.reload(&source) {
                    Ok(report) => {
                        announced = bytes;
                        println!(
                            "reload revision {} preserved {} initialised {} dropped {}",
                            instance.revision(),
                            join_keys(&report.preserved_keys),
                            join_keys(&report.initialised_keys),
                            join_keys(&report.dropped_keys)
                        );
                        if trace {
                            println!(
                                "trace reload revision {} layout {}",
                                instance.revision(),
                                instance.layout_generations()
                            );
                        }
                        if present {
                            if let Some(target) = target {
                                match print_projection(instance, target) {
                                    Ok(nodes) => {
                                        trace_present(trace, nodes, instance.layout_generations());
                                    }
                                    Err(error) => {
                                        eprintln!("{error}");
                                        let _ = io::stderr().flush();
                                    }
                                }
                            }
                        }
                        let _ = io::stdout().flush();
                    }
                    Err(error) => {
                        announced = bytes;
                        eprintln!("{error}");
                        let _ = io::stderr().flush();
                    }
                }
            }
        }
    }
}

fn join_keys(keys: &[String]) -> String {
    if keys.is_empty() {
        "-".to_owned()
    } else {
        keys.join(",")
    }
}

fn trace_sites(enabled: bool, kind: &str, name: &str, sites: usize, layout: u64) {
    if enabled {
        println!("trace {kind} {name} sites {sites} layout {layout}");
    }
}

fn trace_layout(enabled: bool, step: &str, layout: u64) {
    if enabled {
        println!("trace {step} layout {layout}");
    }
}

fn trace_present(enabled: bool, nodes: usize, layout: u64) {
    if enabled {
        println!("trace present closed nodes {nodes} layout {layout}");
    }
}

fn print_projection(
    instance: &mut goya_resolver::Instance,
    target: Target,
) -> Result<usize, goya_resolver::Diagnostic> {
    let nodes = instance.project()?;
    let count = nodes.len();
    println!(
        "present {} {} closed revision {}",
        target.id(),
        target.surface(),
        instance.revision()
    );
    for node in nodes {
        let id = node.id.as_deref().unwrap_or("-");
        let clip = match node.clip {
            Some((x_mp, y_mp, width_mp, height_mp)) => {
                format!(" clip {x_mp},{y_mp},{width_mp},{height_mp}")
            }
            None => String::new(),
        };
        let caret = match node.caret {
            Some(byte) => format!(" caret {byte}"),
            None => String::new(),
        };
        println!(
            "node {id} {} {} {},{},{},{} {:?}{clip}{caret}",
            node.role, node.name, node.x_mp, node.y_mp, node.width_mp, node.height_mp, node.text
        );
    }
    Ok(count)
}

fn required<'a>(args: &'a [String], index: usize, message: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| message.to_owned())
}

fn print_help() {
    println!(
        "\
goya-resolve {} — execute an admitted .gscene without a native compiler

Usage:
  goya-resolve <file.gscene> [options]

Options:
  --validate-only       Admit and resolve, then exit without starting
  --inspect             Accepted; a run always prints the resolved state
  --viewport WxH        Host viewport in points (default 800x600)
  --contract <name>     gssh1, gssh2, gssh3, gssh3-live, gssh4, or gssh4-live
  --target <platform>   android, android_tv, browser, ios, linux, or windows
  --present             Print the projected surface. Requires --target. Does not open it
  --pointer <x,y>       Millipoint hit on that surface. A button activates, an input focuses
  --enter <text>        Replace the focused input's text. Requires --target
  --insert <text>       Insert text at the caret. Requires --target
  --backspace           Delete the grapheme before the caret. Requires --target
  --delete              Delete the grapheme after the caret. Requires --target
  --caret <motion>      left, right, home, or end. Requires --target
  --scroll <x,y,dx,dy>  Scroll the viewport under that point, in millipoints
  --profile <name>      constrained, standard, or performance (recorded, not measured)
  --set <element=text>  Replace a bound field before a later --dispatch
  --dispatch <action>   Run one action by authored id, name, or trigger
  --suspend             Suspend a running instance. Dispatch and input wait
  --resume              Resume a suspended instance
  --watch               Poll the file and reload when its bytes change. Does not exit
  --trace               Print sites evaluated and layout generations. Not a timer
  --benchmark           Not implemented",
        goya_resolver::CONTRACT
    );
}

fn parse_caret(value: &str) -> Result<goya_resolver::CaretMotion, String> {
    match value {
        "left" => Ok(goya_resolver::CaretMotion::Left),
        "right" => Ok(goya_resolver::CaretMotion::Right),
        "home" => Ok(goya_resolver::CaretMotion::Home),
        "end" => Ok(goya_resolver::CaretMotion::End),
        _ => Err(format!("unknown caret {value}")),
    }
}

fn parse_scroll(value: &str) -> Result<(i64, i64, i64, i64), String> {
    let mut parts = value.split(',');
    let take = |parts: &mut std::str::Split<'_, char>, label: &str| -> Result<i64, String> {
        let text = parts
            .next()
            .filter(|text| !text.is_empty())
            .ok_or("--scroll needs x,y,dx,dy millipoints")?;
        text.parse()
            .map_err(|_| format!("invalid scroll {label} {text}"))
    };
    let x_mp = take(&mut parts, "x")?;
    let y_mp = take(&mut parts, "y")?;
    let dx_mp = take(&mut parts, "dx")?;
    let dy_mp = take(&mut parts, "dy")?;
    if parts.next().is_some() {
        return Err("--scroll needs x,y,dx,dy millipoints".to_owned());
    }
    Ok((x_mp, y_mp, dx_mp, dy_mp))
}

fn parse_point(value: &str) -> Result<(i64, i64), String> {
    let (x, y) = value
        .split_once(',')
        .ok_or("--pointer needs x,y millipoints")?;
    let x = x.parse().map_err(|_| format!("invalid pointer x {x}"))?;
    let y = y.parse().map_err(|_| format!("invalid pointer y {y}"))?;
    Ok((x, y))
}

fn parse_viewport(value: &str) -> Result<(f64, f64), String> {
    let split_at = value.find(['x', 'X', ',']).ok_or("--viewport needs WxH")?;
    let width = &value[..split_at];
    let height = &value[split_at + 1..];
    let width: f64 = width
        .parse()
        .map_err(|_| format!("invalid viewport width {width}"))?;
    let height: f64 = height
        .parse()
        .map_err(|_| format!("invalid viewport height {height}"))?;
    Ok((width, height))
}

fn print_instance(instance: &goya_resolver::Instance, performance: PerformanceProfile) {
    println!("contract: {}", instance.semantic_contract());
    print_target(instance.target());
    println!("phase: {}", instance.phase().as_str());
    println!("revision: {}", instance.revision());
    println!("ir: {}", ir_label(instance.ir_lowered()));
    println!("persistence: classified-only");
    println!("performance: {} recorded", performance.as_str());
    for action in instance.actions() {
        println!(
            "action {} {} trigger {}",
            action.id, action.class_name, action.trigger
        );
    }
    for entry in instance.state() {
        println!(
            "state {} {} {} {}",
            entry.key,
            type_name(&entry.value),
            format_value(&entry.value),
            lifetime_name(entry.lifetime)
        );
    }
    for (id, text) in instance.fields() {
        println!("field {id} {text}");
    }
    for display in instance.displays() {
        if let Some(id) = &display.element_id {
            println!("display {id} {}", display.text);
        }
    }
}

fn print_target(target: Option<Target>) {
    if let Some(target) = target {
        println!("platform: {}", target.id());
        println!("compiler: {}", target.compiler());
        println!("surface: {} closed", target.surface());
    }
}

fn ir_label(lowered: bool) -> &'static str {
    if lowered {
        "lowered"
    } else {
        "unavailable"
    }
}

fn type_name(value: &InteractionValue) -> &'static str {
    match value {
        InteractionValue::Int(_) => "int",
        InteractionValue::Text(_) => "text",
        InteractionValue::Bool(_) => "bool",
    }
}

fn format_value(value: &InteractionValue) -> String {
    match value {
        InteractionValue::Int(value) => value.to_string(),
        InteractionValue::Text(value) => format!("{value:?}"),
        InteractionValue::Bool(value) => value.to_string(),
    }
}

fn lifetime_name(lifetime: goya_lang::StateLifetime) -> &'static str {
    match lifetime {
        goya_lang::StateLifetime::Session => "session",
        goya_lang::StateLifetime::Durable => "durable",
        // Refused at admission (`GY-RESOLVER-LIVE`).
        goya_lang::StateLifetime::Host => "host",
    }
}

fn hex(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}
