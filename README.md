# redis-demo
 
## Running
 
Start the server:

```
cargo run --bin redis-server

cargo run --bin redis-server -- --help
```
  
Start the CLI client:

```
cargo run --bin redis-cli set foo bar

cargo run --bin redis-cli get foo
```

 
    
