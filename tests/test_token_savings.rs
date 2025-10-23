use cap_scale::cpu::{Staging, scale_bgra_cpu};
use cap_scale::presets::{AspectMode, ScalePlan, Size, TokenPreset, build_plan};
use fast_image_resize::Resizer;

/// Test token savings from different scaling presets
/// This verifies that our presets actually reduce token usage for VLMs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing Token Savings from Scaling Presets");
    println!("================================================");
    println!("💡 Presets clamp the LONGEST side of the image to a maximum size:");
    println!("   - P2_56, P4, P9, P10_24: max 640px on longest side");
    println!("   - P6_9: max 512px on longest side");
    println!("🔧 Using actual scaling functions from cap-scale crate");
    println!();

    // Create a test image (1920x1080 - common desktop resolution)
    let test_width = 1920;
    let test_height = 1080;
    let bgra_data = create_test_image_bgra(test_width, test_height);

    println!(
        "📏 Test Image: {}x{} ({} pixels)",
        test_width,
        test_height,
        test_width * test_height
    );

    // Test each preset
    let presets = vec![
        (TokenPreset::P2_56_Long640, "P2_56 (640px max)"),
        (TokenPreset::P4_Long640, "P4 (640px max)"),
        (TokenPreset::P6_9_Long512, "P6_9 (512px max)"),
        (TokenPreset::P9_Long640, "P9 (640px max)"),
        (TokenPreset::P10_24_Long640, "P10_24 (640px max)"),
    ];

    // Control: no scaling
    let control_size = Size {
        w: test_width,
        h: test_height,
    };
    let control_tokens = calculate_vision_tokens(control_size);
    println!(
        "🎯 Control (no scaling): {}x{} = {} tokens",
        control_size.w, control_size.h, control_tokens
    );

    println!("\n📊 Preset Results:");
    println!("Preset\t\tMax Side\tOutput Size\tTokens\tSavings\tRatio");

    for (preset, name) in &presets {
        let max_side = match preset {
            TokenPreset::P6_9_Long512 => 512,
            _ => 640,
        };

        let plan = build_plan(
            Size {
                w: test_width,
                h: test_height,
            },
            preset.to_target(),
            AspectMode::Preserve,
        );

        // Actually scale the image using our scaling function
        let scaled_result = apply_scaling(&bgra_data, test_width, test_height, &plan)?;
        let actual_output_size = Size {
            w: scaled_result.width,
            h: scaled_result.height,
        };

        let tokens = calculate_vision_tokens(actual_output_size);
        let savings = control_tokens.saturating_sub(tokens);
        let ratio = control_tokens as f64 / tokens as f64;

        println!(
            "{}\t{}\t\t{}x{}\t{}\t{}\t{:.2}x",
            name, max_side, actual_output_size.w, actual_output_size.h, tokens, savings, ratio
        );
    }

    // Test with a larger image (4K)
    println!("\n🖥️  Testing with 4K image (3840x2160):");
    let test_4k_width = 3840;
    let test_4k_height = 2160;
    let bgra_4k_data = create_test_image_bgra(test_4k_width, test_4k_height);

    let control_4k_size = Size {
        w: test_4k_width,
        h: test_4k_height,
    };
    let control_4k_tokens = calculate_vision_tokens(control_4k_size);
    println!(
        "🎯 Control (no scaling): {}x{} = {} tokens",
        control_4k_size.w, control_4k_size.h, control_4k_tokens
    );

    println!("\n📊 4K Preset Results:");
    println!("Preset\t\tMax Side\tOutput Size\tTokens\tSavings\tRatio");

    for (preset, name) in &presets {
        let max_side = match preset {
            TokenPreset::P6_9_Long512 => 512,
            _ => 640,
        };

        let plan = build_plan(
            Size {
                w: test_4k_width,
                h: test_4k_height,
            },
            preset.to_target(),
            AspectMode::Preserve,
        );

        // Actually scale the 4K image using our scaling function
        let scaled_result = apply_scaling(&bgra_4k_data, test_4k_width, test_4k_height, &plan)?;
        let actual_output_size = Size {
            w: scaled_result.width,
            h: scaled_result.height,
        };

        let tokens = calculate_vision_tokens(actual_output_size);
        let savings = control_4k_tokens.saturating_sub(tokens);
        let ratio = control_4k_tokens as f64 / tokens as f64;

        println!(
            "{}\t{}\t\t{}x{}\t{}\t{}\t{:.2}x",
            name, max_side, actual_output_size.w, actual_output_size.h, tokens, savings, ratio
        );
    }

    // Test with different aspect ratios
    println!("\n📱 Testing with different aspect ratios (all at 1920px longest side):");

    let aspect_ratios = vec![
        ("4:3 Monitor", 1920, 1440),   // 4:3 aspect ratio
        ("21:9 Ultrawide", 1920, 822), // 21:9 aspect ratio
        ("1:1 Square", 1920, 1920),    // 1:1 square
        ("9:16 Portrait", 1080, 1920), // 9:16 portrait (rotated)
    ];

    for (name, width, height) in aspect_ratios {
        println!("\n🎯 {} ({}x{}):", name, width, height);

        let control_size = Size {
            w: width,
            h: height,
        };
        let control_tokens = calculate_vision_tokens(control_size);
        println!(
            "  Control: {}x{} = {} tokens",
            control_size.w, control_size.h, control_tokens
        );

        let bgra_data = create_test_image_bgra(width, height);

        println!("  Preset results:");
        for (preset, preset_name) in &presets {
            let plan = build_plan(
                Size {
                    w: width,
                    h: height,
                },
                preset.to_target(),
                AspectMode::Preserve,
            );

            let scaled_result = apply_scaling(&bgra_data, width, height, &plan)?;
            let actual_output_size = Size {
                w: scaled_result.width,
                h: scaled_result.height,
            };

            let tokens = calculate_vision_tokens(actual_output_size);
            let ratio = control_tokens as f64 / tokens as f64;

            println!(
                "    {}: {}x{} = {} tokens ({:.1}x savings)",
                preset_name, actual_output_size.w, actual_output_size.h, tokens, ratio
            );
        }
    }

    println!("\n✅ Token savings verification complete!");
    println!("💡 This test uses the actual scaling functions from cap-scale crate.");
    println!("   Token counts are calculated from real scaled image dimensions.");

    Ok(())
}

/// Calculate approximate vision tokens for an image
/// This is a simplified calculation based on typical VLM tokenization:
/// - Images are divided into patches (typically 14x14 or 16x16 pixels)
/// - Each patch becomes a fixed number of tokens
/// - Total tokens = (height / patch_size) * (width / patch_size) * tokens_per_patch
fn calculate_vision_tokens(size: Size) -> u64 {
    // Typical VLM parameters (similar to CLIP/SigLIP)
    const PATCH_SIZE: u32 = 14; // Common patch size
    const TOKENS_PER_PATCH: u64 = 4; // Approximate tokens per patch

    // Calculate number of patches in each dimension
    let patches_h = (size.h + PATCH_SIZE - 1) / PATCH_SIZE; // Ceiling division
    let patches_w = (size.w + PATCH_SIZE - 1) / PATCH_SIZE;

    // Total patches
    let total_patches = patches_h as u64 * patches_w as u64;

    // Total tokens (patches * tokens_per_patch + some overhead)
    total_patches * TOKENS_PER_PATCH + 85 // +85 for special tokens (CLS, etc.)
}

/// Create a test image with some content (BGRA format)
fn create_test_image_bgra(width: u32, height: u32) -> Vec<u8> {
    // Create a simple gradient image in BGRA format
    let mut bgra_data = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            let r = (x as f32 / width as f32 * 255.0) as u8;
            let g = (y as f32 / height as f32 * 255.0) as u8;
            let b = 128u8;
            let a = 255u8; // Alpha

            // BGRA order
            bgra_data.push(b); // Blue
            bgra_data.push(g); // Green
            bgra_data.push(r); // Red
            bgra_data.push(a); // Alpha
        }
    }

    bgra_data
}

/// Apply scaling using the actual cap-scale functions
fn apply_scaling(
    input_bgra: &[u8],
    width: u32,
    height: u32,
    plan: &ScalePlan,
) -> Result<ScaledImage, Box<dyn std::error::Error>> {
    // Create resizer (this is how it's done internally)
    let mut resizer = Resizer::new();
    let mut staging = Staging::with_capacity((width * height * 4) as usize);

    // Create output buffer
    let output_size = (plan.out.w * plan.out.h * 4) as usize;
    let mut output = vec![0u8; output_size];

    // Apply scaling
    scale_bgra_cpu(
        &mut resizer,
        input_bgra,
        Size {
            w: width,
            h: height,
        },
        None, // No stride issues for our test image
        plan,
        &mut output,
        Some(&mut staging),
    )?;

    Ok(ScaledImage {
        width: plan.out.w,
        height: plan.out.h,
    })
}

#[derive(Debug)]
struct ScaledImage {
    width: u32,
    height: u32,
}
