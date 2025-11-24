// SPDX-License-Identifier: MIT OR Apache-2.0
use exfiltrate::command::Command;
use exfiltrate_internal::command::{FileInfo, ImageInfo, Response};

struct HelloWorld;
impl Command for HelloWorld {
    fn name(&self) -> &'static str {
        "hello_world"
    }

    fn short_description(&self) -> &'static str {
        "Returns a hello world message.  Use this to test exfiltrate"
    }

    fn full_description(&self) -> &'static str {
        "Returns a hello world message.  Use this to test exfiltrate"
    }

    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        Ok("hello world".into())
    }
}

struct ImageGen;
impl Command for ImageGen {
    fn name(&self) -> &'static str {
        "image_gen"
    }

    fn short_description(&self) -> &'static str {
        "Generates a test image.  Use this to test exfiltrate's image capabilities."
    }

    fn full_description(&self) -> &'static str {
        self.short_description()
    }

    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        let size: u32 = 255;
        let size_u8 = size.try_into().unwrap();
        let mut data = Vec::with_capacity(size as usize * size as usize);
        for r in 0..size_u8 {
            for g in 0..size_u8 {
                let b = 255u8.saturating_sub(r / 2).saturating_sub(g / 2);
                let pixel = rgb::RGBA { r, g, b, a: 255 };
                data.push(pixel);
            }
        }
        Ok(ImageInfo::new(data, size, None).into())
    }
}

struct MultiImageGen;
impl Command for MultiImageGen {
    fn name(&self) -> &'static str {
        "multi_image_gen"
    }

    fn short_description(&self) -> &'static str {
        "Generates multiple test images.  Use this to test exfiltrate's multiple image capabilities."
    }

    fn full_description(&self) -> &'static str {
        self.short_description()
    }

    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        let mut images = Vec::new();

        // Generate 3 different colored images
        for color_shift in 0..3 {
            let size: u32 = 100;
            let size_u8 = size.try_into().unwrap();
            let mut data = Vec::with_capacity(size as usize * size as usize);

            for r in 0..size_u8 {
                for g in 0..size_u8 {
                    let pixel = match color_shift {
                        0 => rgb::RGBA { r, g, b: 0, a: 255 }, // Red-Green gradient
                        1 => rgb::RGBA {
                            r: 0,
                            g,
                            b: r,
                            a: 255,
                        }, // Green-Blue gradient
                        _ => rgb::RGBA {
                            r,
                            g: 0,
                            b: r,
                            a: 255,
                        }, // Red-Blue gradient
                    };
                    data.push(pixel);
                }
            }

            let remark = Some(format!("Test image {}", color_shift + 1));
            images.push(ImageInfo::new(data, size, remark));
        }

        Ok(images.into())
    }
}

struct MultiFileGen;
impl Command for MultiFileGen {
    fn name(&self) -> &'static str {
        "multi_file_gen"
    }

    fn short_description(&self) -> &'static str {
        "Generates multiple test files.  Use this to test exfiltrate's multiple file capabilities."
    }

    fn full_description(&self) -> &'static str {
        self.short_description()
    }

    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        let files = vec![
            FileInfo::new(
                "txt".to_string(),
                Some("First test file".to_string()),
                b"This is the first test file.\nIt has multiple lines.\n".to_vec(),
            ),
            FileInfo::new(
                "txt".to_string(),
                Some("Second test file".to_string()),
                b"This is the second test file.\nWith different content.\n".to_vec(),
            ),
            FileInfo::new(
                "json".to_string(),
                Some("JSON test file".to_string()),
                b"{\"test\": true, \"value\": 123}".to_vec(),
            ),
        ];

        Ok(files.into())
    }
}

fn main() {
    exfiltrate::begin();
    #[cfg(feature = "logwise")]
    {
        logwise::warn_sync!("Hello from logwise!");
    }
    exfiltrate::add_command(HelloWorld);
    exfiltrate::add_command(ImageGen);
    exfiltrate::add_command(MultiImageGen);
    exfiltrate::add_command(MultiFileGen);

    #[cfg(not(target_arch = "wasm32"))]
    std::thread::park();
    //wasm32: atomics.wait cannot be called in this context
}
