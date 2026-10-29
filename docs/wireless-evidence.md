# Wireless protocol findings (static, GNU objdump)

Target: OemDrv.exe SHA256 4091d39ef765b38c26fe77e3afb6f4378f52900edd3a144adb50c661238852ec.
Source: evidence/oem-disassembly.txt (GNU objdump); these annotations are not independently issued REA Evidence IDs. No hardware writes or target execution performed.

## Descriptor-compatible PSD/model query

Actual 3554:fa09 receiver composite exposes input/output report ID 0x13, 19 payload bytes (20 bytes including report ID), and feature ID 6, seven payload bytes. The report6 feature channel is not the 916KB report5 configuration channel.

`Watcher_Thread_3632` starts at VA 0x40c300; call at 0x40c40f invokes generic read command VA0x494d80 with `(handle, report_id=0x13, command=5, destination, length=6, copies=1, attempts=5)`. This requests six PSD bytes, later matched to KB.ini to distinguish F87/F75/F99.

The packet assembled at 0x494dc0..494e11 is twenty bytes:

```
13 05 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 19
```

Fields: [0] report ID; [1] command; [2] packet count (1 on request); [3] packet index 0; [4] request size 0; [5..19) payload zero; [19] wrapping sum of preceding bytes. Checksum helper is VA0x494a90 (sum20 when checksum byte was initialized to0).

VA0x494c50 uses WriteFile with exactly20 bytes; VA0x494ae0 uses ReadFile with exactly20 bytes. Thus Linux hidraw write/read (poll with bounded timeout) is appropriate, not feature ioctl.

Receive parser at 0x494e66 checks `reply[1] & 0x7f == command`; command bit0x80 denotes a device error; `reply[2] & 0x7f` declares packet count, `reply[3]` packet index; `reply[4] & 0x0f` declares data length capped to14 and remaining requested count. Payload starts at index5. It collects sequential packets until six bytes. No receive checksum check was observed in this bounded generic routine; imposing a checksum is an additional implementation choice, not observed original behavior.

The watcher also requests command7 with response length1, then treats byte0=0 as a reason to wait for connection notification. A CLI can issue the direct PSD query with a bounded timeout without reproducing persistent watcher behavior.

## Configuration hooks in same family

CDev3632::SetLED at0x4945f0 invokes AccessData0x493da0 command0x04, index0, caller payload/length, copies1, delay15. GetLED at0x494640 invokes SendCMD0x494130 command0x44, index0, caller payload/length.

SetMatrix at0x494480 uses command1, caller layer index. GetMatrix at0x4944c0 uses0x41. Neither payload layout has been established in this pass.

AccessData0x493da0 fragments into14-byte payloads. Header is `[13, command, total_packet_count, packet_index, (index <<4)|fragment_length, ...14-byte payload..., checksum]`; total packet count ceil(length/14), packet index starts0, checksum is wrapping additive sum preceding19 bytes. It then waits for acknowledgement. This header does not establish LED field offsets or flash persistence semantics.

## Boundary between families

OnNewDongle0x40cc40 explicitly dispatches family0x3632 to constructor0x493a10, family0x8805 to0x49d3f0. G5 GetPSD0x49efe0 uses report9 length520 and is incompatible with currently observed receiver descriptor. CDev916KB report5 command0x81 must not be sent through report6 merely because both share device branding.

OnNewCombo0x40ce40 uses different report IDs5/0x15 and command0x15; it is not descriptor-compatible with this receiver. USB VID/PID matches alone do not establish the entire firmware family; a successful PSD query and checks against observed descriptors remain necessary.

## Grounded effect-field update

Constructor0x493a10 sets vtable0x62220c; slot0x70 is GetLED0x494640, slot0x6c is SetLED0x4945f0. Function0x4950e0 calls GetLED for128 bytes, edits payload[9]=0 and optionally payload[10]=selected hardware effect ID, then SetLED for128 bytes. Buffer base is ESP+8 after saved ESI/EDI; relevant writes at0x49513f (ESP+0x11) and0x495153 (ESP+0x12), hence offsets9 and10. Effect ID comes from the in-memory LedOpt table at base+0x2fbe with40-byte stride, consistent with the explicit hardware effect column of model KB.ini. This establishes an effect-only RMW operation; brightness, color and persistence semantics remain unknown.

## Deep transaction confirmation

`SendCMD`0x494130 sets a query packet with count1/index0 and layer nibble in byte4, then receives all fragments. For LED query this is `13 44 01 00 00` +14 zeros +checksum58. It ignores previously seen fragment indices (0x4942e0), copies from reply+5 using length low nibble (0x49430d), and tracks all declared fragments. The final128-byte response requires10 distinct fragments. `AccessData`0x493da0 sends ceil(N/14) individual packets and waits for matching ACK after every normal command (cmd6 and high-bit commands have special no-ACK paths irrelevant to LED4). It validates ACK index against sent index at0x494002..49400b.

ACK helper0x493910 checks received byte0==FF for disconnect, byte2&80 for rejection, byte1 exact command match, and returns byte3 as packet index. The discovery routine0x494d80 additionally treats high bit in command byte1 as error. For normal initialized commands, byte2 low7 bits are fragment count. Do not confuse these byte2 status semantics with the command byte1 status check in the simpler discovery path.

`protocol.rs` implements output/read transactions with bounded deadlines, matching command/report, strict indices/counts, complete data, padded final payload support, duplicate consistency, per-fragment write ACK validation, and known PSD check before configuration. Six protocol tests verify known PSD/query bytes, reordered/duplicated replies, malformed replies, final padding, ten-fragment128-byte write/ACK stop, and preserved unknown LED fields. Tests use synthetic packets, not captured hardware responses; static compatibility has not been exercised on hardware.
