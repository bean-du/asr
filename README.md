



# Download Metal Shader

```bash
curl -o resources/ggml-metal.metal https://raw.githubusercontent.com/ggerganov/whisper.cpp/v1.5.4/ggml-metal.metal
```

# Build for Mac with Metal Support

```
cargo build --release --features metal
```