#!/usr/bin/env python3
"""
M12: 下载 pix2text-mfr (TrOCR) ONNX 模型并验证推理性能

模型：breezedeus/pix2text-mfr (HuggingFace)
架构：TrOCR (Vision Encoder + Text Decoder)
文件：
- encoder_model.onnx (87.5 MB)
- decoder_model.onnx (30.1 MB)
- tokenizer.json (39 KB)
- config.json / preprocessor_config.json

验证步骤：
1. 下载模型文件
2. 创建测试公式图片
3. 运行 ONNX 推理（encoder → decoder 自回归）
4. 输出 LaTeX 结果 + 耗时
"""

import os
import sys
import time
import json
import urllib.request
import numpy as np
from PIL import Image, ImageDraw, ImageFont

MODELS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "models", "formula")

# HuggingFace 上 pix2text-mfr 模型
MODEL_URLS = {
    "encoder_model.onnx": "https://huggingface.co/breezedeus/pix2text-mfr/resolve/main/encoder_model.onnx",
    "decoder_model.onnx": "https://huggingface.co/breezedeus/pix2text-mfr/resolve/main/decoder_model.onnx",
    "tokenizer.json": "https://huggingface.co/breezedeus/pix2text-mfr/resolve/main/tokenizer.json",
    "config.json": "https://huggingface.co/breezedeus/pix2text-mfr/resolve/main/config.json",
    "preprocessor_config.json": "https://huggingface.co/breezedeus/pix2text-mfr/resolve/main/preprocessor_config.json",
}


def download_models():
    """下载模型文件"""
    os.makedirs(MODELS_DIR, exist_ok=True)

    for filename, url in MODEL_URLS.items():
        filepath = os.path.join(MODELS_DIR, filename)
        if os.path.exists(filepath):
            size_mb = os.path.getsize(filepath) / 1024 / 1024
            print(f"  ✓ {filename} already exists ({size_mb:.1f} MB)")
            continue

        print(f"  ⬇ Downloading {filename} ...")
        try:
            urllib.request.urlretrieve(url, filepath)
            size_mb = os.path.getsize(filepath) / 1024 / 1024
            print(f"  ✓ {filename} downloaded ({size_mb:.1f} MB)")
        except Exception as e:
            print(f"  ✗ Failed to download {filename}: {e}")
            return False

    return True


def create_test_formula_image(text="E=mc²", width=384, height=64):
    """创建测试公式图片（白底黑字）"""
    img = Image.new("RGB", (width, height), (255, 255, 255))
    draw = ImageDraw.Draw(img)
    try:
        font = ImageFont.truetype("/System/Library/Fonts/Times.ttc", 28)
    except Exception:
        font = ImageFont.load_default()

    text_bbox = draw.textbbox((0, 0), text, font=font)
    text_w = text_bbox[2] - text_bbox[0]
    text_h = text_bbox[3] - text_bbox[1]
    x = (width - text_w) // 2
    y = (height - text_h) // 2
    draw.text((x, y), text, fill=(0, 0, 0), font=font)
    return img


def preprocess_image(img, target_size=(384, 384)):
    """TrOCR 预处理: RGB → resize → normalize → NCHW"""
    img_resized = img.resize(target_size, Image.LANCZOS)
    img_array = np.array(img_resized, dtype=np.float32) / 255.0

    # ImageNet normalization
    mean = np.array([0.5, 0.5, 0.5])
    std = np.array([0.5, 0.5, 0.5])
    img_array = (img_array - mean) / std

    # HWC → NCHW
    img_array = np.transpose(img_array, (2, 0, 1))
    img_array = np.expand_dims(img_array, 0).astype(np.float32)
    return img_array


def run_inference():
    """使用 onnxruntime 运行推理"""
    import onnxruntime as ort

    # 加载配置
    config_path = os.path.join(MODELS_DIR, "config.json")
    with open(config_path, "r") as f:
        config = json.load(f)

    preproc_path = os.path.join(MODELS_DIR, "preprocessor_config.json")
    with open(preproc_path, "r") as f:
        preproc_config = json.load(f)

    print(f"\n  Model config:")
    print(f"    Architecture: {config.get('architectures', ['unknown'])}")
    print(f"    Encoder: {config.get('encoder', {}).get('model_type', 'unknown')}")
    print(f"    Decoder: {config.get('decoder', {}).get('model_type', 'unknown')}")
    print(f"    Vocab size: {config.get('decoder', {}).get('vocab_size', 'unknown')}")
    print(f"    Max length: {config.get('max_length', 'unknown')}")

    img_size = preproc_config.get("size", {})
    img_h = img_size.get("height", 384)
    img_w = img_size.get("width", 384)
    print(f"    Image size: {img_w}x{img_h}")

    # 加载 tokenizer
    tokenizer_path = os.path.join(MODELS_DIR, "tokenizer.json")
    with open(tokenizer_path, "r") as f:
        tokenizer_data = json.load(f)

    vocab = tokenizer_data.get("model", {}).get("vocab", {})
    id2token = {v: k for k, v in vocab.items()}

    # 特殊 token
    added_tokens = tokenizer_data.get("added_tokens", [])
    special_map = {}
    for t in added_tokens:
        special_map[t.get("content", "")] = t.get("id", -1)

    sos_id = special_map.get("</s>", special_map.get("<s>", 2))
    eos_id = special_map.get("</s>", 2)
    pad_id = special_map.get("<pad>", 1)

    # TrOCR 使用 decoder_start_token_id
    decoder_start_id = config.get("decoder_start_token_id", sos_id)

    print(f"\n  Tokenizer:")
    print(f"    Vocab size: {len(vocab) + len(added_tokens)}")
    print(f"    SOS/decoder_start: {decoder_start_id}")
    print(f"    EOS: {eos_id}")
    print(f"    PAD: {pad_id}")

    # 加载模型
    print("\n  Loading ONNX models...")
    load_start = time.time()

    encoder_session = ort.InferenceSession(
        os.path.join(MODELS_DIR, "encoder_model.onnx"),
        providers=["CPUExecutionProvider"],
    )
    decoder_session = ort.InferenceSession(
        os.path.join(MODELS_DIR, "decoder_model.onnx"),
        providers=["CPUExecutionProvider"],
    )
    load_time = (time.time() - load_start) * 1000
    print(f"  Models loaded in {load_time:.0f}ms")

    # 打印 I/O 信息
    print("\n  === Encoder I/O ===")
    for inp in encoder_session.get_inputs():
        print(f"    Input:  {inp.name} shape={inp.shape} type={inp.type}")
    for out in encoder_session.get_outputs():
        print(f"    Output: {out.name} shape={out.shape} type={out.type}")

    print("\n  === Decoder I/O ===")
    for inp in decoder_session.get_inputs():
        print(f"    Input:  {inp.name} shape={inp.shape} type={inp.type}")
    for out in decoder_session.get_outputs():
        print(f"    Output: {out.name} shape={out.shape} type={out.type}")

    # 测试推理
    test_cases = ["E=mc²", "∑xᵢ²", "∫f(x)dx", "a²+b²=c²"]

    for formula_text in test_cases:
        print(f"\n  --- Test: '{formula_text}' ---")
        img = create_test_formula_image(formula_text, img_w, img_h)
        img_input = preprocess_image(img, (img_w, img_h))

        total_start = time.time()

        try:
            # Encoder
            enc_input_name = encoder_session.get_inputs()[0].name
            enc_start = time.time()
            enc_output = encoder_session.run(None, {enc_input_name: img_input})
            enc_time = (time.time() - enc_start) * 1000

            encoder_hidden = enc_output[0]
            print(f"    Encoder: {enc_time:.1f}ms, hidden shape: {encoder_hidden.shape}")

            # Decoder (autoregressive)
            dec_inputs = decoder_session.get_inputs()
            dec_input_names = [i.name for i in dec_inputs]
            print(f"    Decoder inputs: {dec_input_names}")

            dec_start = time.time()
            generated = [decoder_start_id]
            max_len = config.get("max_length", 256)

            for step in range(min(max_len, 128)):
                tgt = np.array([generated], dtype=np.int64)

                feed = {}
                for di in dec_inputs:
                    name = di.name
                    if "encoder_hidden_states" in name:
                        feed[name] = encoder_hidden
                    elif "input_ids" in name:
                        feed[name] = tgt
                    elif "encoder_attention_mask" in name:
                        # 全 1 mask
                        seq_len = encoder_hidden.shape[1]
                        feed[name] = np.ones((1, seq_len), dtype=np.int64)
                    elif "attention_mask" in name:
                        feed[name] = np.ones_like(tgt, dtype=np.int64)
                    else:
                        print(f"      Unknown input: {name}")

                step_out = decoder_session.run(None, feed)
                logits = step_out[0]  # [1, seq_len, vocab_size]

                next_logits = logits[0, -1, :]
                next_token = int(np.argmax(next_logits))

                if next_token == eos_id:
                    break

                generated.append(next_token)

            dec_time = (time.time() - dec_start) * 1000
            total_time = (time.time() - total_start) * 1000

            # 解码
            latex_tokens = []
            for tid in generated[1:]:
                token = id2token.get(tid, f"<{tid}>")
                latex_tokens.append(token)

            latex = "".join(latex_tokens)
            # 清理 BPE Ġ 等
            latex = latex.replace("Ġ", " ").strip()

            num_tokens = len(generated) - 1
            per_token_ms = dec_time / max(num_tokens, 1)

            print(f"    Decoder: {dec_time:.1f}ms ({num_tokens} tokens, {per_token_ms:.1f}ms/tok)")
            print(f"    Total:   {total_time:.1f}ms")
            print(f"    LaTeX:   {latex}")

        except Exception as e:
            total_time = (time.time() - total_start) * 1000
            print(f"    ✗ Error after {total_time:.1f}ms: {e}")
            import traceback
            traceback.print_exc()


def main():
    print("=" * 60)
    print("M12: pix2text-mfr (TrOCR) ONNX Model Validation")
    print("=" * 60)

    print("\n[1/3] Downloading models...")
    if not download_models():
        print("Failed to download models, aborting.")
        sys.exit(1)

    print("\n[2/3] Model files:")
    total_size = 0
    for f in sorted(os.listdir(MODELS_DIR)):
        fp = os.path.join(MODELS_DIR, f)
        size = os.path.getsize(fp)
        total_size += size
        print(f"  {f}: {size / 1024 / 1024:.1f} MB")
    print(f"  Total: {total_size / 1024 / 1024:.1f} MB")

    print("\n[3/3] Running inference test...")
    run_inference()

    print("\n" + "=" * 60)
    print("Validation complete!")
    print("=" * 60)


if __name__ == "__main__":
    main()
