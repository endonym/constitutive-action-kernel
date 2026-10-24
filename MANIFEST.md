# Kernel Manifest

This Manifest describes the constitutive-action kernel source distributed in this repository. Applications and deployments disclose their own operational control.

The kernel evaluates an exact proposed effect against a host-supplied, versioned decision basis and returns a decision. Integrating hosts authenticate participants, resolve applicable conditions, verify evidence and hold effectful capabilities. The repository distributes the library; the integrating application operates the service.

An integrating application controls its authority resolver, condition and evidence sources, persistence, dispatch reservation and effectful adapter. Those are the places where it can combine rules of participation with execution. The operator must describe any fusion under its control in a deployment-specific Manifest, including who controls it and how participants or users can reproduce or contest it.

The kernel's behavior can be inspected and reproduced from the [source](src/v1.rs), [contract](docs/contract.md), [host integration guide](docs/host-integration.md), [threat model](docs/threat-model.md), [examples](examples) and [tests](tests). Run `cargo test --locked --all-targets` against a checkout with Rust 1.99 or later. The repository's GitHub issue tracker provides a route to contest or propose changes to this source; each deployed integration provides its own accessible process.
