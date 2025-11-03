# Guidelines to build and test on QNX target

## 1. Build and Install QNX Rust toolchain

**Prerequisites:**

a. Host OS: Ubuntu 24.04

b. QNX7.1 SDP installed

**Build & Install:**

Execute the script to build and install QNX7.1 rust toolchain.

```sh
./inc_orchestrator_internal/scripts/build_qnx_rust_toolchain.sh
```

**Note:** If needed, change paths and rust version in the script before build.

## 2. Build Rust application for x86_64/aarch64 QNX

#### a. Set QNX environment variables.
```sh
source ~/qnx710/qnxsdp-env.sh
```

#### b. Build for desired target (x86_64-pc-nto-qnx710 / aarch64-unknown-nto-qnx710).
Build using xtask. Example for `x86_64-pc-nto-qnx710`,
```sh
cargo xtask build:qnx_x86_64 --example basic
```

For `aarch64-unknown-nto-qnx710`
```sh
cargo xtask build:qnx_arm --example basic
```

**or**

Set toolchain and target on command line during build.
```sh
cargo +qnx7.1_rust build --target x86_64-pc-nto-qnx710 --example basic
```

**Note:** Bazel build is not supported yet.

## 3. Testing
### a. Create QEMU-QNX image
#### i) Create an empty directory (ex: qnx7.1_image) and switch to it

#### ii) Set QNX environment variables
```sh
source ~/qnx710/qnxsdp-env.sh
```
#### iii) Create image for x86_64
```sh
mkqnximage --build --hostname=QNX_7.1_TEST --type=qemu --arch=x86_64 --data-size=5000 --data-inodes=40000
```
#### iv) Edit configuration file for SSH and rebuild image
Open `./local/snippets/ifs_files.custom` and add the line
`/usr/libexec/sshd-session=usr/libexec/sshd-session` and rebuild image
```sh
mkqnximage --build --hostname=QNX_7.1_TEST --type=qemu --arch=x86_64 --data-size=5000 --data-inodes=40000
```
#### v) Run image (qemu)
```sh
mkqnximage --run
```
**Note:**
Press `Ctrl+a` then `x` to terminate qemu instance.

### b. Transfer application and execute
#### vi) Open new terminal and set QNX environment variables
```sh
source ~/qnx710/qnxsdp-env.sh
```
#### vii) Get IP address of the qemu (qnx) instance
```sh
mkqnximage --getip
```
#### viii) Transfer application using SCP. Example,
```sh
scp <application_binary> root@<ip_addres>:/data/home/root/
```
#### ix) Execute applicatoin
Goto `/data/home/root/` in the running qemu instance and execute the application.

**or**

Remote login into running qemu instance and then execute the application.
```sh
ssh root@<ip_address>
```
password is same as user name `root`
