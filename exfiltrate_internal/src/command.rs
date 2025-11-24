use rgb::RGBA8;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// A command that can be executed by the exfiltrate system.
///
/// Commands are the primary way to expose functionality to the exfiltrate CLI.
/// Each command has a unique name, descriptions, and an execution method.
pub trait Command: 'static + Send + Sync {
    /// The unique name of the command.
    ///
    /// This is used to invoke the command from the CLI (e.g. `exfiltrate <name>`).
    fn name(&self) -> &'static str;

    /// A short, one-line description of the command.
    ///
    /// This is shown in the `list` command.
    fn short_description(&self) -> &'static str;

    /// A full description of the command.
    ///
    /// This is shown when `help <name>` is invoked. It should include usage instructions.
    fn full_description(&self) -> &'static str;

    /// Executes the command with the given arguments.
    ///
    /// Returns a `Response` on success, or a `Response` (usually a string error) on failure.
    fn execute(&self, args: Vec<String>) -> Result<Response, Response>;
}

/**
A response from a command
*/
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Response {
    String(String),
    Bytes(Vec<u8>),
    Files(Vec<FileInfo>),
    Images(Vec<ImageInfo>),
}

/// Information about a file to be returned to the client.
#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    /// The proposed extension for the file (e.g. "txt", "png").
    /// The client will generate a random filename with this extension.
    pub proposed_extension: String,
    /// An optional remark or description to display to the user.
    pub remark: Option<String>,
    /// The binary contents of the file.
    pub contents: Vec<u8>,
}

/// Information about an image to be returned to the client.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ImageInfo {
    /// The raw image data in RGBA8 format.
    pub data: Vec<RGBA8>,
    /// The width of the image in pixels.
    pub width: u32,
    /// The height of the image in pixels.
    pub height: u32,
    /// An optional remark or description to display to the user.
    pub remark: Option<String>,
}

impl ImageInfo {
    pub fn new(data: Vec<RGBA8>, width: u32, remark: Option<String>) -> ImageInfo {
        assert!(
            data.len().is_multiple_of(width as usize),
            "width is incorrect for this array"
        );
        let data_len: u32 = data.len().try_into().unwrap();
        let height: u32 = data_len / width;
        ImageInfo {
            data,
            width,
            height,
            remark,
        }
    }
}

impl Display for ImageInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Image {}x{}", self.width, self.height))
    }
}

impl FileInfo {
    pub fn new(proposed_extension: String, remark: Option<String>, contents: Vec<u8>) -> FileInfo {
        FileInfo {
            proposed_extension,
            remark,
            contents,
        }
    }
}

impl Display for FileInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.proposed_extension)?;
        match self.remark {
            Some(ref remark) => {
                write!(f, "file: {},", remark)?;
            }
            None => {
                write!(f, "file,")?;
            }
        }
        write!(f, "{} bytes", self.contents.len())
    }
}

impl Response {
    pub fn into_string(self) -> String {
        match self {
            Response::String(s) => s,
            Response::Bytes(_bytes) => todo!(),
            Response::Files(_files) => todo!(),
            Response::Images(_images) => todo!(),
        }
    }

    pub fn from_serialize<S: Serialize>(s: &S) -> Result<Response, Response> {
        match rmp_serde::to_vec(s) {
            Ok(bytes) => Ok(Response::Bytes(bytes)),
            Err(e) => Err(Response::String(e.to_string())),
        }
    }

    pub fn split_data(&mut self) -> Vec<Vec<u8>> {
        match self {
            Response::String(_) => vec![],
            Response::Bytes(b) => {
                let data = std::mem::take(b);
                vec![data]
            }
            Response::Files(files) => {
                let mut parts = Vec::new();
                for file in files {
                    parts.push(std::mem::take(&mut file.contents));
                }
                parts
            }
            Response::Images(images) => {
                let mut parts = Vec::new();
                for image in images {
                    // Convert RGBA8 to raw bytes
                    // This assumes RGBA8 is 4 bytes and safe to cast, which it is (#[repr(C)] usually, or just a struct of 4 u8s)
                    // But wait, Vec<RGBA8> -> Vec<u8> isn't a simple cast without unsafe or iteration.
                    // Let's iterate for safety and simplicity first, or use bytemuck if available.
                    // Since I can't add dependencies easily, I'll just iterate.
                    // Actually, let's use unsafe to cast Vec<RGBA8> to Vec<u8> if alignment matches,
                    // but RGBA8 comes from `rgb` crate.
                    // Let's just flatten it.
                    let mut data = Vec::with_capacity(image.data.len() * 4);
                    for pixel in &image.data {
                        data.push(pixel.r);
                        data.push(pixel.g);
                        data.push(pixel.b);
                        data.push(pixel.a);
                    }
                    image.data.clear();
                    parts.push(data);
                }
                parts
            }
        }
    }

    pub fn merge_data(&mut self, parts: Vec<Vec<u8>>) {
        // We need to consume parts in the same order they were produced
        // Since we push to `parts` in order, we should probably reverse it to pop, or use an iterator.
        // Let's use an iterator.
        let mut parts_iter = parts.into_iter();

        match self {
            Response::String(_) => {}
            Response::Bytes(b) => {
                if let Some(data) = parts_iter.next() {
                    *b = data;
                }
            }
            Response::Files(files) => {
                for file in files {
                    if let Some(data) = parts_iter.next() {
                        file.contents = data;
                    }
                }
            }
            Response::Images(images) => {
                for image in images {
                    if let Some(data) = parts_iter.next() {
                        // Reconstruct Vec<RGBA8>
                        let num_pixels = data.len() / 4;
                        let mut pixels = Vec::with_capacity(num_pixels);
                        for chunk in data.chunks_exact(4) {
                            pixels.push(RGBA8::new(chunk[0], chunk[1], chunk[2], chunk[3]));
                        }
                        image.data = pixels;
                    }
                }
            }
        }
    }
}

impl From<String> for Response {
    fn from(s: String) -> Self {
        Response::String(s)
    }
}

impl From<&str> for Response {
    fn from(s: &str) -> Self {
        Response::String(s.to_string())
    }
}

impl From<Vec<FileInfo>> for Response {
    fn from(files: Vec<FileInfo>) -> Self {
        Response::Files(files)
    }
}

impl From<Vec<ImageInfo>> for Response {
    fn from(images: Vec<ImageInfo>) -> Self {
        Response::Images(images)
    }
}

impl From<FileInfo> for Response {
    fn from(file: FileInfo) -> Self {
        Response::Files(vec![file])
    }
}

impl From<ImageInfo> for Response {
    fn from(image: ImageInfo) -> Self {
        Response::Images(vec![image])
    }
}

impl Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Response::String(s) => write!(f, "{}", s),
            Response::Bytes(bytes) => write!(f, "<{} bytes>", bytes.len()),
            Response::Files(files) => {
                for (i, file) in files.iter().enumerate() {
                    if i > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{}", file)?;
                }
                Ok(())
            }
            Response::Images(images) => {
                for (i, img) in images.iter().enumerate() {
                    if i > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{}", img)?;
                }
                Ok(())
            }
        }
    }
}
