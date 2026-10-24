# ROS message schema

Both transports (`rosbridge` and `rclrs`) use `std_msgs/msg/String` messages
whose `data` field is UTF-8 JSON. The topics come from `NodeConfig`:

| Topic | Direction | Payload |
| --- | --- | --- |
| `/robot/capabilities` | bridge -> ROS | `Capability` |
| `/robot/tasks` | ROS -> bridge | `Task` |
| `/robot/proofs` | bridge -> ROS | `Proof` |

The schema is the serde representation of the types in
`crates/bridge-core/src/types.rs`; unknown fields are ignored for forward
compatibility.

## Capability

```json
{
  "id": "navigate",
  "version": 1,
  "units": "per_metre",
  "metadata": { "max_speed_mps": 1.2, "frame": "map" }
}
```

- `id`: 1-64 characters, letters/digits/`-`/`_`.
- `version`: schema version for this capability.
- `units`: human-readable pricing unit.
- `metadata`: free-form JSON for the robot's advert.

## Task

```json
{
  "id": 42,
  "capability": "navigate",
  "params": { "x": 1.5, "y": 2.5 },
  "reward": { "asset": { "kind": "native" }, "amount": 10000000 },
  "deadline": 1792566000,
  "requester": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
}
```

- `id`: the on-chain `TaskId` assigned by `TaskEscrow.create_task`.
- `reward.amount`: stroops (`i128`); `reward.asset` is
  `{"kind":"native"}` or `{"kind":"sac","sac":"C..."}`.
- `deadline`: Unix timestamp in seconds; the contract rejects proofs at or
  after it.
- `requester`: Stellar account strkey (`G...`).

## Proof

```json
{
  "task_id": 42,
  "digest": "078b2f0c1c0e6f4b1d5a8f8c...",
  "signer": "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
  "signature": "0000...0000"
}
```

- `digest`: 32-byte SHA-256 telemetry digest, encoded as 64 lowercase hex
  characters.
- `signer`: the executor account that submitted the proof on-chain.
- `signature`: 64-byte signature as 128 hex characters. The example robot emits
  zeros to make clear it is a simulation; production robots must sign the digest
  with the signer key.

## Example publish

```bash
ros2 topic pub --once /robot/tasks std_msgs/msg/String \
  "{data: '{\"id\":42,\"capability\":\"navigate\",\"params\":{\"x\":1.5,\"y\":2.5},\"reward\":{\"asset\":{\"kind\":\"native\"},\"amount\":10000000},\"deadline\":1792566000,\"requester\":\"GAAAA...\"}'}"
```
