# HOW TO USE

## server

    cargo run <your can>(such as can0) <ipaddr>(such as 127.0.0.1:8080,default_value = "127.0.0.1:8080")

## client 

### start motor

    cargo run <your can>(such as can0) <addr>(such as "ws://127.0.0.1:8080",default_value = "ws://127.0.0.1:8080"") --command --data <your data-float>

### stop motor

    cargo run <your can>(such as can0) <addr>(such as "ws://127.0.0.1:8080",default_value = "ws://127.0.0.1:8080"") --data <your data-float>