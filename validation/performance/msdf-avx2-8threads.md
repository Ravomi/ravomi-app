# MSDF 8 线程 AVX2 编译对照

> 历史性能证据：以下保留原环境、语料、哈希和结果，仅说明该次字形生成实验，不是新版 UI、整帧性能或 Agent 协作验收。当前可读性与交互验收依据为 [DESIGN](../../DESIGN.md)、[AI_UI_RULES](../../AI_UI_RULES.md)，产品范围见 [PRD](../../PRD.md)。新界面复测须另查文字锐度、相同输入下的生成成本和真实画布运行，不把装饰纹理加入文字渲染路径。

2026-10-10，本机 Intel Core i9-13900H，Linux x86_64，rustc 1.98.1 (48a229cea 2026-09-01)。

## 结论

仅打开编译器 AVX2 指令支持，没有观察到稳定收益。本实验没有手写汇编、重写 SIMD 内核、启用 FMA，也没有改变应用默认构建配置或字形线程数。结果不能推断手工向量化的上限或画布帧率。

| 编译方式 | 工作线程 | 次数 | 中位数 ms | 最小 ms | 最大 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| release 默认指令集 | 8 | 9 | 302.012478 | 298.313011 | 312.828459 |
| release `-C target-feature=+avx2` | 8 | 9 | 301.243477 | 283.655658 | 325.272194 |

中位耗时减少约 0.25%，小于此次测量波动。桌面后台应用仍在运行，未固定 CPU 亲和性，混合核心调度、温度和频率变化没有被完全隔离。单次约 0.3 秒，仅作为初步对照，不是严格性能承诺。

## 输入与一致性

- 输入：[固定测试文本](msdf-avx2-corpus.txt)，282 个不同字符、282 个字形，无缺字。与历史 632 字形语料不同，不能直接比较绝对耗时。
- 字体：`/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc`，face 0。
- 64 texels/em，距离范围 8；每轮重新生成字形。
- 每轮输出 4,346,612 RGBA 字节，校验和均为 `4dda19b0215ab64d`；18 次结果一致（校验和比较，不是逐字节比对）。
- 字体 SHA-256：`b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a`。
- 文本 SHA-256：`32ecfda3892b936f45d3da7b4d66eeb4c1d86b69c2ec594ec3ee9e0d22534931`。
- `src/msdf/gen.rs` SHA-256：`890863d89e6501c5130f7e7688b45016cb20069ba0769279140e7aa7f60e3a95`（相对字形库目录）。
- `src/msdf/edge_segment/cubic.rs` SHA-256：`2292089a7d9c9ad182ea35d41f3d76cbbcf555e960c0d5020d4bfe3fb8aea74a`。

原始结果：[默认](msdf-avx2-baseline.jsonl)、[AVX2](msdf-avx2-avx2.jsonl)。每份文件包含 3 个进程的输出，每个进程运行 3 次，所以 trial 编号会重复。

## 方法与复现

将 `vendor/kayak_font_msdf/` 复制到 workspace 外的临时目录，同时复制根 `Cargo.lock`。仅在临时目录把 `examples/cold_glyphs.rs` 的 `for threads in [1, 4, 8]` 改为 `for threads in [8]`；生成器源码不变。

在临时目录执行：

```sh
cargo build --release --offline --example cold_glyphs --target-dir target/baseline
RUSTFLAGS='-C target-feature=+avx2' cargo build --release --locked --offline --example cold_glyphs --target-dir target/avx2
```

首次独立构建会从复制的 workspace 锁文件中裁去不再使用的包，未升级字形库依赖。两版共用同一份裁剪后的锁文件。

完成编译和测试后，用上述固定文本的绝对路径作为 TEXT，字体路径作为 FONT，依次运行 baseline、avx2、avx2、baseline、baseline、avx2：

```sh
target/baseline/release/examples/cold_glyphs TEXT FONT
target/avx2/release/examples/cold_glyphs TEXT FONT
```

每个进程内部自动运行 3 轮。计时包括生成、线程创建/汇合和结果哈希，不含字体文件读取、应用布局及 GPU 上传。两版串行运行，计时期间没有由本实验启动的编译任务。

使用 `objdump -d -C -M intel` 检查 AVX2 二进制，发现 `vpermpd ymm` 等 AVX2 指令；三次曲线 `signed_distance` 的检查片段仍主要使用 XMM 宽度运算。这不证明整条像素循环已经批量向量化。

## 检查

两版分别通过 `cargo test --release --locked --offline --target-dir ... -- --include-ignored --test-threads=1`，每版 5 项测试，包含系统中文字体轮廓测试。临时字形库的 `cargo fmt --check` 和默认版 release 全 targets Clippy `-D warnings` 通过；原始 JSON 已检查样本数、线程数、字节数和校验和。

本次项目内变更仅为技能、来源说明和验证工件；没有修改应用 Rust 源码，没有重跑整个桌面 workspace 的测试。
