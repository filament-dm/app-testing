This is a testing and development app for the Matrix Rust SDK. This is particularly useful as a test bed to understand the SDK before bringing it into a mobile app via Uniffi, which has a much longer dev debug cycle.

## Setup

Create a `config.yaml` file patterned after `config.yaml.example`. Then run Cargo:

```
$ cargo run
```

The app will log in as the configured user and output a variety of messages. If you initiate emoji session verification from Element, the app will respond and automatically accept and confirm verification.


### Matrix SDK Update

A copy of the Matrix Rust SDK has been subtree'd into this repo at `matrix-rust-sdk/`. In order to update it, you'll need to do some [subtree magic](https://www.atlassian.com/git/tutorials/git-subtree). The procedure is:

1. Set a git remote to the upstream Rust repo: `git remote add matrix-rust-sdk git@github.com:matrix-org/matrix-rust-sdk.git` but DO NOT RUN A FETCH.

2. Update with the subtree command from the root of the repo: `git subtree pull --prefix matrix-rust-sdk/ matrix-rust-sdk main --squash`.

3. This will put you into a git merge cycle -- if you have made changes in `matrix-rust-sdk/` then you'll need to resolve conflicts and complete the merge. After this, you will have an updated sdk in a merge commit on HEAD. This can be rolled back via git if necessary.

While this app uses a snapshot of the public Matrix repo, so that we can share it out with bug repros, it's possible with a little elbow grease to point this at forks of the SDK.


### UI

We capture keyboard input (see `keyboard.rs`) and output to the logger. The app outputs what keys it responds to at startup.


### Timeline Testing

If you specify a `timeline_test_room` room id in `config.yaml`, the app will construct a matrix-sdk-ui timeline object a retrieve a short history of messages. If `timeline_wait_verification` is true then it does this *after* successful e2e verification, which must be initiated from another Matrix client. If wait verification is false then the client will construct the timeline immediately after start. You can back paginate through this timeline by hitting `p`.
