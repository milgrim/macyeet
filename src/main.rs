use std::path::PathBuf;
use std::sync::OnceLock;

use clap::Parser;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, AnyProtocol, Bool, ClassBuilder, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, AnyThread, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::*;
use objc2_foundation::*;

static ARGS: OnceLock<Cli> = OnceLock::new();

#[derive(Parser, Debug)]
#[command(name = "yeet", about = "Yeet files into any app via native macOS drag and drop", version)]
struct Cli {
    /// Paths to files to yeet
    #[arg(value_name = "PATH", required = true)]
    paths: Vec<PathBuf>,

    /// Exit after first successful drag
    #[arg(short = 'x', long)]
    and_exit: bool,
}

// ── App Delegate ──────────────────────────────────────────────

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "YeetDelegate"]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _: &NSNotification) {
            let mtm = MainThreadMarker::new().unwrap();
            let args = ARGS.get().unwrap();

            let row_height = 24.0_f64;
            let padding = 10.0_f64;
            let size_col_w = 70.0_f64;
            let time_col_w = 115.0_f64;
            let gap = 8.0_f64;

            // Collect file info and measure name widths
            struct FileInfo {
                name: String,
                size: String,
                time: String,
            }
            let files: Vec<FileInfo> = args.paths.iter().map(|path| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                let (size, time) = std::fs::metadata(path)
                    .map(|m| {
                        let sz = m.len();
                        let size_str = if sz >= 1_073_741_824 {
                            format!("{:.1} GB", sz as f64 / 1_073_741_824.0)
                        } else if sz >= 1_048_576 {
                            format!("{:.1} MB", sz as f64 / 1_048_576.0)
                        } else if sz >= 1024 {
                            format!("{:.1} KB", sz as f64 / 1024.0)
                        } else {
                            format!("{} B", sz)
                        };
                        let time_str = m.modified().ok().map(|t| {
                            let dt: std::time::Duration = t
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default();
                            let secs = dt.as_secs() as i64;
                            // Convert to local time using libc
                            let tm = unsafe {
                                let mut result = std::mem::zeroed::<libc::tm>();
                                libc::localtime_r(&secs, &mut result);
                                result
                            };
                            format!(
                                "{:04}-{:02}-{:02} {:02}:{:02}",
                                tm.tm_year + 1900,
                                tm.tm_mon + 1,
                                tm.tm_mday,
                                tm.tm_hour,
                                tm.tm_min,
                            )
                        }).unwrap_or_default();
                        (size_str, time_str)
                    })
                    .unwrap_or_default();
                FileInfo { name, size, time }
            }).collect();

            // Estimate name column width (~7.5px per char for system font ~13pt)
            let max_name_chars = files.iter().map(|f| f.name.len()).max().unwrap_or(10);
            let name_col_w = (max_name_chars as f64 * 7.5).max(100.0);
            let win_width = (padding + name_col_w + gap + size_col_w + gap + time_col_w + padding)
                .clamp(300.0, 800.0);
            let win_height = ((files.len() as f64) * row_height + padding * 2.0).max(60.0);

            let window = unsafe {
                let w: Retained<NSWindow> = msg_send![
                    mtm.alloc::<NSWindow>(),
                    initWithContentRect: NSRect::new(
                        NSPoint::new(0.0, 0.0),
                        NSSize::new(win_width, win_height),
                    ),
                    styleMask: NSWindowStyleMask::Titled.union(NSWindowStyleMask::Closable),
                    backing: 2u64,
                    defer: false,
                ];
                w.setReleasedWhenClosed(false);
                w.center();
                let n = files.len();
                let title = if n == 1 {
                    "yeet - 1 file".to_string()
                } else {
                    format!("yeet - {} files", n)
                };
                w.setTitle(&NSString::from_str(&title));
                w
            };

            let cls = AnyClass::get(c"YeetDragView").unwrap();
            let view: Retained<NSView> = unsafe {
                let obj: *mut AnyObject = msg_send![cls, alloc];
                let obj: *mut NSView = msg_send![obj, initWithFrame: NSRect::new(
                    NSPoint::new(0.0, 0.0),
                    NSSize::new(win_width, win_height),
                )];
                Retained::from_raw(obj).unwrap()
            };

            let right_edge = win_width - padding;
            for (i, file) in files.iter().enumerate() {
                let y = win_height - padding - (i as f64 + 1.0) * row_height;

                // Name (left-aligned)
                let name_label = NSTextField::labelWithString(
                    &NSString::from_str(&file.name), mtm,
                );
                name_label.setFrame(NSRect::new(
                    NSPoint::new(padding, y),
                    NSSize::new(name_col_w, row_height - 2.0),
                ));
                view.addSubview(&name_label);

                // Size (right-aligned)
                let size_label = NSTextField::labelWithString(
                    &NSString::from_str(&file.size), mtm,
                );
                size_label.setAlignment(NSTextAlignment::Right);
                size_label.setFrame(NSRect::new(
                    NSPoint::new(right_edge - time_col_w - gap - size_col_w, y),
                    NSSize::new(size_col_w, row_height - 2.0),
                ));
                view.addSubview(&size_label);

                // Time (right-aligned)
                let time_label = NSTextField::labelWithString(
                    &NSString::from_str(&file.time), mtm,
                );
                time_label.setAlignment(NSTextAlignment::Right);
                time_label.setFrame(NSRect::new(
                    NSPoint::new(right_edge - time_col_w, y),
                    NSSize::new(time_col_w, row_height - 2.0),
                ));
                view.addSubview(&time_label);
            }

            window.setContentView(Some(&view));
            window.makeKeyAndOrderFront(None);

            let app = NSApplication::sharedApplication(mtm);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_terminate(&self, _: &NSApplication) -> bool {
            true
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc();
        unsafe { msg_send![this, init] }
    }
}

// ── Drag View (ClassBuilder for full method control) ──────────

fn start_drag(this: *mut AnyObject, event: *mut AnyObject) {
    let args = ARGS.get().unwrap();
    let mut items: Vec<Retained<NSDraggingItem>> = Vec::new();

    for path in &args.paths {
        let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        // Use NSURL so the path is properly percent-encoded (handles spaces, etc.)
        let url_str = unsafe {
            let path_ns = NSString::from_str(&abs_path.to_string_lossy());
            let file_url: Retained<NSURL> =
                msg_send![NSURL::class(), fileURLWithPath: &*path_ns];
            let abs_str: Retained<NSString> = msg_send![&*file_url, absoluteString];
            abs_str.to_string()
        };

        let pb_item: Retained<NSPasteboardItem> =
            unsafe { msg_send![NSPasteboardItem::alloc(), init] };
        unsafe {
            let _: bool = msg_send![
                &*pb_item,
                setString: &*NSString::from_str(&url_str),
                forType: &*NSString::from_str("public.file-url"),
            ];
        }

        let drag_item: Retained<NSDraggingItem> = unsafe {
            msg_send![
                NSDraggingItem::alloc(),
                initWithPasteboardWriter: &*pb_item,
            ]
        };
        unsafe {
            drag_item.setDraggingFrame_contents(
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(32.0, 32.0)),
                None,
            );
        }
        items.push(drag_item);
    }

    let items_refs: Vec<&NSDraggingItem> = items.iter().map(|i| &**i).collect();
    let items_array = NSArray::from_slice(&items_refs);

    unsafe {
        let _: *mut AnyObject = msg_send![
            this,
            beginDraggingSessionWithItems: &*items_array,
            event: event,
            source: this,
        ];
    }
}

// All extern "C" fns use raw pointers to satisfy MethodImplementation lifetime bounds
extern "C" fn mouse_down(this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    start_drag(this, event);
}

extern "C" fn key_down(_this: *mut AnyObject, _cmd: Sel, event: *mut AnyObject) {
    let chars: Option<Retained<NSString>> = unsafe { msg_send![event, characters] };
    if let Some(chars) = chars {
        let s = chars.to_string();
        if s == "q" || s == "Q" || s == "\u{1b}" {
            std::process::exit(0);
        }
    }
}

extern "C" fn accepts_first_responder(_this: *mut AnyObject, _cmd: Sel) -> Bool {
    Bool::YES
}

extern "C" fn accepts_first_mouse(
    _this: *mut AnyObject,
    _cmd: Sel,
    _event: *mut AnyObject,
) -> Bool {
    Bool::YES
}

extern "C" fn source_op_mask(
    _this: *mut AnyObject,
    _cmd: Sel,
    _session: *mut AnyObject,
    _context: NSInteger,
) -> NSUInteger {
    1 // NSDragOperationCopy
}

extern "C" fn drag_ended(
    _this: *mut AnyObject,
    _cmd: Sel,
    _session: *mut AnyObject,
    _point: NSPoint,
    operation: NSUInteger,
) {
    if ARGS.get().unwrap().and_exit && operation != 0 {
        std::process::exit(0);
    }
}

fn register_drag_view_class() {
    let superclass = NSView::class();
    let mut builder = ClassBuilder::new(c"YeetDragView", superclass).unwrap();

    if let Some(proto) = AnyProtocol::get(c"NSDraggingSource") {
        builder.add_protocol(proto);
    }

    unsafe {
        builder.add_method(sel!(mouseDown:), mouse_down as extern "C" fn(_, _, _));
        builder.add_method(sel!(keyDown:), key_down as extern "C" fn(_, _, _));
        builder.add_method(
            sel!(acceptsFirstResponder),
            accepts_first_responder as extern "C" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(acceptsFirstMouse:),
            accepts_first_mouse as extern "C" fn(_, _, _) -> _,
        );
        builder.add_method(
            sel!(draggingSession:sourceOperationMaskForDraggingContext:),
            source_op_mask as extern "C" fn(_, _, _, _) -> _,
        );
        builder.add_method(
            sel!(draggingSession:endedAtPoint:operation:),
            drag_ended as extern "C" fn(_, _, _, _, _),
        );
    }

    builder.register();
}

// ── Main ──────────────────────────────────────────────────────

fn main() {
    let args = Cli::parse();
    for path in &args.paths {
        if !path.exists() {
            eprintln!("yeet: {}: No such file or directory", path.display());
            std::process::exit(1);
        }
    }
    ARGS.set(args).expect("Could not set ARGS");

    register_drag_view_class();

    let mtm = MainThreadMarker::new().expect("must run on main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

    app.run();
}
