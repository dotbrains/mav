# Cli

## Testing

You can test your changes to the `cli` crate by first building the main mav binary:

```
cargo build -p mav
```

And then building and running the `cli` crate with the following parameters:

```
 cargo run -p cli -- --mav ./target/debug/mav.exe
```
