//! Test-only process adapter. The GUI application's bridge stays in-process.
use lipa_core::layout::{
    WrapMode,
    fit::{FontSpec, Measured, TextMeasure},
};
use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
};

pub struct QtMeasure {
    child: RefCell<Child>,
    output: RefCell<BufReader<std::process::ChildStdout>>,
}

impl QtMeasure {
    pub fn from_env() -> Option<Self> {
        let executable = std::env::var_os("LIPAX_QT_MEASURE_SERVER")?;
        let fonts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/assets/fonts");
        let mut child = Command::new(executable)
            .arg(fonts)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start Qt measurement server");
        let output = BufReader::new(child.stdout.take().expect("Qt measurement stdout"));
        Some(Self {
            child: RefCell::new(child),
            output: RefCell::new(output),
        })
    }

    fn request(&self, request: serde_json::Value) -> serde_json::Value {
        let mut child = self.child.borrow_mut();
        let input = child.stdin.as_mut().expect("Qt measurement stdin");
        writeln!(input, "{request}").expect("write Qt measurement request");
        input.flush().expect("flush Qt measurement request");
        let mut reply = String::new();
        self.output
            .borrow_mut()
            .read_line(&mut reply)
            .expect("read Qt measurement response");
        serde_json::from_str(&reply).expect("valid Qt measurement response")
    }
}

impl TextMeasure for QtMeasure {
    fn measure(
        &self,
        text: &str,
        font: &FontSpec,
        px: f32,
        letter_spacing: f32,
        width: f32,
        wrap: WrapMode,
    ) -> Measured {
        let wrap = match wrap {
            WrapMode::WordWrap => 0,
            WrapMode::WrapAnywhere => 1,
            WrapMode::NoWrap | WrapMode::Elide => 2,
        };
        let r = self.request(serde_json::json!({"operation":"measure", "text":text, "family":font.family,
            "weight":font.weight.value(), "italic":font.italic, "px":px, "letter_spacing":letter_spacing, "width":width, "wrap":wrap}));
        Measured {
            width: r["width"].as_f64().unwrap() as f32,
            height: r["height"].as_f64().unwrap() as f32,
            line_spacing: r["line_spacing"].as_f64().unwrap() as f32,
            lines: r["lines"].as_u64().unwrap() as u32,
        }
    }
    fn cap_ratio(&self, font: &FontSpec) -> f32 {
        self.request(serde_json::json!({"operation":"cap", "family":font.family, "weight":font.weight.value(),
            "italic":font.italic, "px":100.0, "letter_spacing":0.0}))["cap_ratio"].as_f64().unwrap() as f32
    }
}

impl Drop for QtMeasure {
    fn drop(&mut self) {
        let child = self.child.get_mut();
        child.stdin.take();
        child.wait().expect("reap Qt measurement server");
    }
}
