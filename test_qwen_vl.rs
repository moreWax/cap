use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use reqwest::blocking::Client;
use std::time::Duration;

fn main() -> Result<()> {
    println!("Testing Qwen3-VL API integration with dummy image...");

    // Create a simple dummy PNG image (1x1 pixel transparent PNG)
    let dummy_png = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, // IHDR chunk length
        0x49, 0x48, 0x44, 0x52, // IHDR
        0x00, 0x00, 0x00, 0x01, // width: 1
        0x00, 0x00, 0x00, 0x01, // height: 1
        0x08, 0x06, 0x00, 0x00,
        0x00, // bit depth: 8, color type: 6 (RGBA), compression: 0, filter: 0, interlace: 0
        0x1F, 0xF3, 0xFF, 0x61, // IHDR CRC
        0x00, 0x00, 0x00, 0x0C, // IDAT chunk length
        0x49, 0x44, 0x41, 0x54, // IDAT
        0x78, 0x9C, 0x62, 0x60, 0x60, 0x00, 0x00, 0x00, 0x04, 0x00, 0x01, // compressed data
        0x27, 0x6B, 0xB1, 0x42, // IDAT CRC
        0x00, 0x00, 0x00, 0x00, // IEND chunk length
        0x49, 0x45, 0x4E, 0x44, // IEND
        0xAE, 0x42, 0x60, 0x82, // IEND CRC
    ];

    let base64_image = general_purpose::STANDARD.encode(&dummy_png);

    println!("Testing Qwen3-VL API integration...");

    let image_data = format!("data:image/png;base64,{}", base64_image);
    let input_text = format!(
        "This is a test of RTSP streaming integration with screen capture. Please describe what you see in this test image: {}",
        image_data
    );

    let client = Client::new();
    let request_body = serde_json::json!({
        "input": input_text
    });

    let response = client
        .post("http://192.168.1.5:8001/v1/responses")
        .json(&request_body)
        .timeout(Duration::from_secs(30))
        .send();

    match response {
        Ok(resp) => {
            if resp.status().is_success() {
                let result: serde_json::Value = resp.json()?;
                println!("✓ Qwen3-VL API responded successfully");
                println!("Response: {}", result);
            } else {
                println!("✗ API returned error status: {}", resp.status());
                let error_text = resp.text().unwrap_or_default();
                println!("Error details: {}", error_text);
            }
        }
        Err(e) => {
            println!("✗ Network error connecting to Qwen3-VL API: {}", e);
            println!("Make sure the model server is running at http://192.168.1.5:8001");
        }
    }

    println!("Test completed.");
    Ok(())
}
