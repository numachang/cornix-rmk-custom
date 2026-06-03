# 開発メモ（WS2812 ステータスLED & BLE 接続表示）

Cornix（nRF52840 BLE split・RMK ファーム）の実装で得た知見のまとめ。
ハマりどころと、その回避策を残す。詳細は [`src/ws2812.rs`](../src/ws2812.rs) を参照。

---

## WS2812 ステータスLED

各半に 2 個（inner = pixel0 / outer = pixel1）。GRB 順、調光レベル `0x10`。

### 駆動方式：PWM（EasyDMA）

**最終的に PWM 周辺＋EasyDMA で駆動**。これが正解（CPU を波形生成ループに入れない＝
割り込みを止めない＝BLE 無線を一切邪魔しない）。

- `SequencePwm::new_1ch(PWM0, data_pin, config)` を各半に 1 個。
  `Prescaler::Div1`（16MHz）, `max_duty = 20`（COUNTERTOP, 1bit=1.25µs）, `SequenceLoad::Common`。
- 各 WS2812 ビット = PWM 1 周期。シーケンス語は embassy `DutyCycle` の raw 値で、
  **bit15=1（inverted）で「先頭から n tick High」**。`0x8000 | n`：
  - `0` → `0x8000 | 6`（6/20≈0.375µs High）、`1` → `0x8000 | 13`（13/20≈0.81µs High）。
- 2px × 3byte × 8bit = 48 語＋末尾に低レベル語 ~40（≈50µs）でリセット/ラッチ。
- 再生は `SingleSequencer::new(&mut pwm, &buf, cfg).start(Times(1))`。DMA 完了を待つため
  `Timer::after(1ms)` してから `seq`/`buf` を drop（buf は再生中に解放しないこと）。
- **IRQ も `interrupt::free` も DWT もレジスタ直叩きも不要。**

#### 不採用にした手法（再挑戦防止のため記録）

- **CPU ビットバンギング**：`interrupt::free` で割り込みを ~60µs 止めるため MPSL を阻害し、
  数分で BLE 切断＋コントローラ異常停止＝**フリーズ**。さらに DWT ビジーwait が省電力時に
  止まると割り込み停止中に**永久ハング**。タイミングも `asm::delay` 不正確・配線差で白化など、
  根本的に脆い。
- **SPIM(DMA)**：`SPIM3` は HFCLK 駆動で MPSL が無線非活動時に止め転送未完了→ハング。
  `SPIM2` の `new_txonly_nosck` でも本基板では出力が出ず。

### ラグ／消灯の設計（PWM でも維持）

- インジケータは「イベントで点灯 → 一定時間後に消灯」のワンショット。通常時（接続済みアイドル）は
  両ピクセル消灯。
- `render` は **色が変わった時だけ** DMA 転送（WS2812 はラッチ保持）。アイドル時は何もしない。
- PWM 化で割り込みを一切止めないため、そもそも LED 動作が無線に影響しない（フリーズ・ラグの根絶）。

---

## BLE 接続インジケータ（central の outer ピクセル）

### 仕様

- 接続できるまで：プロファイル色で**点滅**（検索、約 10 サイクルで停止）。
- 接続成立：**緑（プロファイル色）を約 2.5 秒点灯 → 消灯**。
- プロファイル色：`0 = 緑 / 1 = 赤 / 2 = 青`。
- peripheral 側の outer は split リンク状態（青）を表示。inner は電池/充電/peer-loss。

### 接続検出のハマりどころ

RMK には「BLE 接続 LED」の参考実装が無い（組み込み controller はロック LED / battery / wpm のみ）。
接続検出は自前で行うが、素直な手段が軒並み使えなかった：

1. **`BleState::Connected` イベントは不安定。**
   GATT の `ConnectionParamsUpdated` / `DataLengthUpdated` が契機のため、ホストによっては
   遅延（接続の数秒後）または未発火。これだけに頼ると緑パルスが出ない/遅れる。
2. **`rmk::state::get_connection_state()`（`CONNECTION_STATE`）は「ホスト接続」を表さない。**
   広告中に走る `run_keyboard` がこれを `Connected` にするため、接続前から `Connected` を返す。
   ポーリングすると接続前に誤って緑パルスが出る。
3. **採用：`ControllerEvent::KeyboardIndicator`（ホストの LED 出力レポート）を接続エッジに使う。**
   出力レポートは接続が確立していなければ届かない＝誤検出なし。多くのホスト（Windows 等）は
   接続時に初期 LED 同期レポートを送る。`ble_connected` が false の時だけ発火させ、
   接続後の CapsLock 等での再発火を抑止する。`BleState::Connected` も併用（出るホスト用）。

### 二重パルス対策

パルス/点滅タイマ（`frame`）のリセットは **立ち上がりエッジのみ**で行う
（connected が false→true、advertising が false→true、プロファイル変更）。
立下りでリセットすると、接続の数秒後に遅れて来る `BleState::Connected` が
`advertising` フラグを落とす変化を拾い、**緑パルスが 2 回出る**。

---

## ピン / 構成（参考）

| 項目 | 左 (central) | 右 (peripheral) |
|---|---|---|
| WS2812 data | P0.24 | P0.13 |
| ext-power (active-high) | P0.13 | P0.24 |

- storage：`0xA0000` + 32 sectors（ファーム本体・bootloader と非衝突）。
- 詳細・キーマップは [`keyboard.toml`](../keyboard.toml) を参照。
