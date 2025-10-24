# Screen Capture Library Development Todos

## Phase 1: Core Infrastructure (Foundation)
- [x] Implement Scaling Processor
  - Implement scaling processor in CaptureSessionBuilder::with_scaling() - currently just returns self without adding any scaling processor to the pipeline
- [x] Complete File Stream Implementation
  - Complete FileStream implementation in processing.rs - has basic structure but may need full GStreamer pipeline integration and error handling
- [ ] Implement Graceful Shutdown
  - Implement Graceful Shutdown - Add graceful shutdown mechanism to CaptureSession - implement proper cleanup of streams, pipelines, and resources on shutdown
- [ ] Add Error Handling and Recovery
  - Add Error Handling and Recovery - Add comprehensive error handling throughout the codebase - many functions have basic error handling but could benefit from more detailed error types and recovery strategies

## Phase 2: CLI/EGUI Integration (Enable Testing)
- [ ] Add Session-Based Capture Mode to CLI
  - Add Session-Based Capture Mode to CLI - Add session-based capture mode to CLI - replace direct capture_screen() calls with CaptureSessionBuilder pattern to enable processing pipelines and multiple streams
- [ ] Add Session-Based Capture to EGUI
  - Add Session-Based Capture to EGUI - Add session-based capture to EGUI app - replace direct capture_screen() with CaptureSessionBuilder to enable processing pipelines
- [ ] Add Scaling Preset Selection to CLI
  - Add Scaling Preset Selection to CLI - Add scaling preset selection to CLI - add --scale-preset flag with dropdown for TokenPreset options (P2_56, P4, P6_9, P9, P10_24) to enable VLM input optimization
- [ ] Add Gundam Mode Toggle to CLI
  - Add Gundam Mode Toggle to CLI - Add Gundam mode toggle to CLI - add --gundam flag to enable DeepSeek-OCR tiling mode for document analysis
- [ ] Add Scaling Preset Selection to EGUI
  - Add Scaling Preset Selection to EGUI - Add scaling preset dropdown to EGUI - add combo box for TokenPreset selection (P2_56, P4, P6_9, P9, P10_24) in the UI
- [ ] Add Gundam Mode Toggle to EGUI
  - Add Gundam Mode Toggle to EGUI - Add Gundam mode checkbox to EGUI - add checkbox to enable/disable DeepSeek-OCR tiling mode

## Phase 3: Core Feature Testing (Validate Foundation)
- [x] Test Scaling Processor Integration
  - Test Scaling Processor Integration - Test CaptureSessionBuilder::with_scaling() - verify scaling processor is added to pipeline with TokenPreset::P4_Long640 (chosen for typical VLM input optimization), TokenPreset::P2_56_Long640 (minimum scaling), TokenPreset::P10_24_Long640 (maximum scaling) - verify pipeline contains scaling processor and output size changes correctly
- [x] Test Gundam Processor Integration
  - Test Gundam Processor Integration - Test CaptureSessionBuilder::with_gundam() - verify GundamProcessor is added to pipeline, initialize() calculates correct composite dimensions for 1920x1080 input (chosen as common screen resolution), verify process_frame() produces expected tile count and global view - ensures Gundam OCR optimization works
- [ ] Test Capture Session Execution
  - Test Capture Session Execution - Test CaptureSession::run() - verify session initializes pipeline and multiplexer, processes frames through all configured processors and streams - test with mock capture source to verify end-to-end processing flow

## Phase 4: Enhanced Features (Build Upon Working Foundation)
- [ ] Add Performance Monitoring
  - Add Performance Monitoring - Implement performance monitoring and metrics collection - add counters for frame rates, processing times, memory usage, and stream statistics
- [ ] Add Performance Monitoring Display to EGUI
  - Add Performance Monitoring Display to EGUI - Add real-time performance monitoring to EGUI - add panel showing live metrics (fps, memory usage, stream statistics, buffer utilization)
- [ ] Add Performance Monitoring to CLI
  - Add Performance Monitoring to CLI - Add performance monitoring display to CLI - add --stats flag to show real-time metrics (fps, memory usage, stream stats) during capture
- [ ] Add Multiple Output Support to CLI
  - Add Multiple Output Support to CLI - Add multiple output support to CLI - add flags for simultaneous RTSP + file output, multiple RTSP streams with different ports/configs
- [ ] Add Multiple Output Configuration to EGUI
  - Add Multiple Output Configuration to EGUI - Add multiple output configuration to EGUI - add UI for configuring simultaneous RTSP + file outputs, multiple RTSP streams
- [ ] Add Configuration File Support
  - Add Configuration File Support - Add configuration file support (TOML/JSON) for persistent settings - currently only supports command-line arguments
- [ ] Add Configuration File Support to CLI
  - Add Configuration File Support to CLI - Add configuration file support to CLI - add --config flag to load TOML/JSON config files for persistent settings
- [ ] Add Configuration File Support to EGUI
  - Add Configuration File Support to EGUI - Add configuration file load/save to EGUI - add buttons to load from and save to TOML/JSON config files
- [ ] Add Stream Management UI to EGUI
  - Add Stream Management UI to EGUI - Add stream management UI to EGUI - add controls for starting/stopping individual streams, viewing stream status, managing connections
- [ ] Add Processing Pipeline Visualization to EGUI
  - Add Processing Pipeline Visualization to EGUI - Add processing pipeline visualization to EGUI - add diagram showing active processors (scaling, gundam) and their configurations

## Phase 5: Comprehensive Testing (Validate Everything)
- [ ] Test RTSP Stream Configuration
  - Test RTSP Stream Configuration - Test CaptureSessionBuilder::with_rtsp_stream() - verify RTSP stream is added with port 8554 (default RTSP port), width/height 1920x1080 (HD resolution), fps 30 (common streaming rate) - verify stream config matches parameters and publisher is created
- [ ] Test File Output Stream Configuration
  - Test File Output Stream Configuration - Test CaptureSessionBuilder::with_file_output() - verify FileStream is added with path 'test.mp4', dimensions 1920x1080, fps 30 - verify stream config is correct and FileStream instance is properly configured
- [ ] Test Pipeline Initialization
  - Test Pipeline Initialization - Test ProcessingPipeline::initialize() - verify sequential processor initialization with Size{w:1920,h:1080} input, check output size propagation through multiple processors (Gundam + scaling), verify error handling when processor fails
- [ ] Test Frame Processing Pipeline
  - Test Frame Processing Pipeline - Test ProcessingPipeline::process_frame() - verify frame flows through all processors in order, test with 1920x1080 BGRA frame (common screen size), verify output frame dimensions match final processor, test frame skipping when processor returns None
- [ ] Test Gundam Processor Initialization
  - Test Gundam Processor Initialization - Test GundamProcessor::initialize() - test with Size{w:1920,h:1080} (FHD), Size{w:2560,h:1440} (QHD), Size{w:3840,h:2160} (4K) - verify tile grid calculation (cols=3,rows=3 for FHD), output dimensions = cols*tile_side x rows*tile_side - ensures proper grid layout for different resolutions
- [ ] Test Gundam Frame Processing
  - Test Gundam Frame Processing - Test GundamProcessor::process_frame() - verify tile extraction from 1920x1080 BGRA frame, check tile count matches grid calculation (9 tiles), verify global view downscaling, test composite arrangement produces expected output dimensions - ensures OCR optimization works correctly
- [ ] Test Buffer Pool Operations
  - Test Buffer Pool Operations - Test BufferPool::get_buffer() and return_buffer() - test with buffer_size=8192 (8KB, typical frame size), max_buffers=4 (reasonable pool size), verify buffer reuse after return, test pool overflow behavior, verify zeroing on return prevents data leakage
- [ ] Test Buffer Pool Statistics
  - Test Buffer Pool Statistics - Test BufferPool::stats() - verify available/max counts after get/return operations, test with empty pool (0/4), full pool operations, verify stats don't affect pool state - ensures monitoring doesn't interfere with performance
- [ ] Test Ring Buffer Operations
  - Test Ring Buffer Operations - Test RingBuffer::write_frame() and read_frame() - test with frame_size=4096, capacity=10, verify buffer full/empty detection, test data integrity through write/read cycle, verify atomic position updates prevent race conditions
- [ ] Test Ring Buffer Status
  - Test Ring Buffer Status - Test RingBuffer::status() - verify available/total frame counts, test with empty buffer (0/10), partially filled (5/10), full buffer (10/10), verify status accuracy during concurrent operations - ensures proper buffer utilization monitoring
- [ ] Test Configuration Validation
  - Test Configuration Validation - Test CaptureOptions::validate() - test valid configs (fps=30, seconds=60, crf=23), invalid fps=0 (should fail), invalid seconds=0 (should fail), invalid crf=10 (too low), invalid crf=30 (too high) - verify all constraints are properly enforced
- [ ] Test Duration Parsing
  - Test Duration Parsing - Test parse_duration() - test '30s' (30), '2m' (120), '1h' (3600), '45' (45 raw seconds), invalid 'abc' (should fail), empty string (should fail) - verify flexible duration parsing works correctly
- [ ] Test Quality Preset Parsing
  - Test Quality Preset Parsing - Test parse_quality() - test 'medium' (23), 'low' (28), 'high' (20), 'ultra' (18), case insensitive 'MEDIUM' (23), invalid 'invalid' (should fail) - verify quality preset mapping and error handling
- [ ] Test Scrap FFmpeg Capture
  - Test Scrap FFmpeg Capture - Test capture_scrap_ffmpeg() - test full screen capture with options{fps:30, seconds:1, crf:23}, window capture (interactive, requires user input), scaling enabled with P4 preset, gundam mode (should fail gracefully) - verify FFmpeg integration and parameter handling
- [ ] Test GStreamer Wayland Capture
  - Test GStreamer Wayland Capture - Test capture_gstreamer() - test with CaptureOptions{fps:30, seconds:1, crf:23}, verify XDG Portal integration, GStreamer pipeline creation, frame streaming - test Wayland capture functionality end-to-end
- [ ] Test RTSP Frame Publishing
  - Test RTSP Frame Publishing - Test RtspPublisher::send() - test normal operation with valid BgraFrame, test back-pressure handling with full queue (should retry), test disconnected channel (should error) - verify thread-safe frame submission with proper error handling
- [ ] Test File Stream Operations
  - Test File Stream Operations - Test FileStream::initialize() and send_frame() - test GStreamer pipeline creation with MP4 output, verify frame encoding and file writing, test pipeline state management - ensure file output works correctly

## Phase 6: Advanced Features (Future Enhancements)
- [ ] Add Multiple Stream Support
  - Add Multiple Stream Support - Add support for multiple concurrent RTSP streams with different configurations - currently limited to single stream per server
- [ ] Add Adaptive Bitrate
  - Add Adaptive Bitrate - Implement adaptive bitrate streaming based on network conditions and client capabilities
- [ ] Implement Stream Security
  - Implement Stream Security - Add authentication and access control for RTSP streams - currently open to any client
- [ ] Add H.265 Encoding Support
  - Add H.265 Encoding Support - Add support for H.265/HEVC encoding for better compression efficiency and bandwidth savings
- [ ] Implement GPU Acceleration
  - Implement GPU Acceleration - Add GPU acceleration support via wgpu for faster scaling operations on supported hardware
- [ ] Add Frame Dropping Strategies
  - Add Frame Dropping Strategies - Add frame dropping strategies for sustained overload conditions to maintain real-time performance
- [ ] Implement Connection Management
  - Implement Connection Management - Add client connection limits and connection management for RTSP server
- [ ] Add Progressive JPEG Support
  - Add Progressive JPEG Support - Add support for progressive JPEG encoding to reduce network bandwidth for web applications
- [ ] Implement Logging System
  - Implement Logging System - Add comprehensive logging and debugging facilities throughout the codebase
- [ ] Add Unit and Integration Tests
  - Add Unit and Integration Tests - Add comprehensive test suite covering all major components and edge cases
- [ ] Implement Additional Capture Sources
  - Implement Additional Capture Sources - Add support for additional capture sources beyond screen capture (camera, video files, etc.)
- [ ] Add Multiple Output Formats
  - Add Multiple Output Formats - Add support for additional output formats beyond MP4 and RTSP (WebRTC, HLS, etc.)
- [ ] Implement Web Management Interface
  - Implement Web Management Interface - Add web-based management interface for monitoring and controlling capture sessions
- [ ] Add Cloud Storage Support
  - Add Cloud Storage Support - Add support for recording to cloud storage services (AWS S3, Azure Blob, etc.)