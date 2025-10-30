load("@score_virtualization//virtualization:virtualization_build.bzl", "virtualization_build")

virtualization_build(
    name = "virtualization",
    srcs = [
        "//src/orchestration:basic",
        "//src/orchestration:catch_error",
        "//src/orchestration:inter_process_event_receiver",
        "//src/orchestration:inter_process_event_sender",
    ],
    outs = [
        "custom/basic",
        "custom/catch_error",
        "custom/inter_process_event_receiver",
        "custom/inter_process_event_sender",
    ],
    disable = True,
    init_script = "custom_init.sh",
)
