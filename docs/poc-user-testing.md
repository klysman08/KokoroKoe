# Manual-question POC user test

This proof of concept tests one real path: a user chooses a finalized saved-transcript segment, sends a bounded text-only question through OpenRouter, and receives a locally validated transient answer.

Use non-sensitive test meeting content. Although audio never leaves the device, the disclosure shown before submission identifies the bounded transcript and instruction text that will be sent to OpenRouter.

## Prepare the app

1. Build the locked x64 release executable. This POC is still a no-bundle build and does not contain the native Whisper runtimes.
2. Close KokoroKoe, then stage the already verified external runtimes with `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-poc-transcription-runtime.ps1`. The script copies and byte/hash-verifies the five-file CPU runtime beside `kokorokoe.exe` and the six-file Vulkan runtime under its private `vulkan` subdirectory. It never copies a model, audio, transcript, credential, or database. If either verified P3 runtime is stored elsewhere, pass `-RuntimeDirectory` for CPU and `-VulkanRuntimeDirectory` for Vulkan.
3. Launch `src-tauri/target/x86_64-pc-windows-msvc/release/kokorokoe.exe` on Windows 10 22H2 or Windows 11 x64.
4. In **Settings**, choose a local workspace folder.
5. Install and select a local transcription model if one is not already available.
6. In **OpenRouter credential**, save a test API key, select **Validate credential**, and load or refresh the privacy-filtered model catalog.
7. In **Preferences**, select a **Manual questions model**, set a small nonzero **Default session budget (USD)**, and save. Only privacy-qualified text models from the loaded catalog are offered.
8. Create a new project and a new session after saving those defaults. The model and budget are frozen into the new session; an older session may not have a manual-question model.

## Exercise the POC

1. Start the new session after confirming the recording/transcription consent notice.
2. Produce a short, non-sensitive exchange through the microphone or system output, then stop the session so finalized segments are saved.
3. From the project session list, select **View transcript**.
4. On one segment, select **Ask about this segment**.
5. Read the **What leaves this device** disclosure. Ask a question whose answer should be present in that moment or its nearby context, then select **Ask OpenRouter**.
6. Confirm that the UI shows a plain-text answer, any limitations, token use, attempt count, repair state when applicable, and the process-local Session cost.
7. Ask once about an early segment and once about a late segment. Check that answers do not claim access to unrelated distant meeting content.

If the app says the catalog is required, return to **Settings**, refresh the OpenRouter models, and try again. The completion boundary intentionally performs no implicit catalog network refresh.

If the app reports `manual_question_provider_requirements_unavailable`, the selected model currently has no provider endpoint that satisfies KokoroKoe's ZDR, data-collection-denial, and structured-output requirements. Refresh the catalog and select a different manual-question model; do not relax privacy settings merely to clear this error. `manual_question_provider_temporarily_unavailable` means retry or choose another model. Timeout, network, authentication, payment, invalid-request, and provider-availability failures now use separate sanitized codes instead of the old generic `manual_question_provider_unavailable` path.

On Windows x64 with a usable Vulkan device, a Session first starts the private supervised worker and accepts work only after that child attests Vulkan. If worker startup or inference fails, KokoroKoe terminates it and retries the same finalized utterance exactly once through the separately staged CPU runtime. CPU-only hosts therefore remain supported.

If starting a session reports `live_transcription_runtime_unavailable`, close the app and repeat the runtime-staging command from **Prepare the app**. If a later build replaces the release directory, restage both DLL sets before testing. Developers can prove that the product result came from Vulkan rather than CPU by running `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-product-live-transcription.ps1 -RequireVulkan`; that gate supplies an intentionally invalid CPU adapter, so a matching partial/final can only come from the attested worker.

## Record feedback

Capture the following without copying API keys or sensitive transcript text:

- Windows version and whether CPU or Vulkan local transcription was used.
- OpenRouter model identifier and approximate answer latency.
- Whether the selected moment and nearby context were sufficient and factually represented.
- Whether the external-service disclosure was clear before submission.
- The visible attempt/repair indicators and any sanitized error code/reference.
- Any confusing interaction, inaccessible control, freeze, or unexpected content exposure.

## POC limitations

- Answers, limitations, and usage presentation are transient; changing the question/segment, closing the transcript, or restarting the app clears them.
- Usage accounting is process-local and resets to the Session's persisted baseline after restart. Treat the displayed budget as a POC guard, not a durable billing cap across restarts; also use an OpenRouter account limit.
- A submitted request has no UI cancellation command or progress stream. The action remains disabled until the bounded blocking operation returns.
- Only saved finalized segments are eligible. Proactive insights, whole-Session summaries, generated-result persistence, and packaged installer validation are outside this POC.
- The native CPU and Vulkan transcription DLLs are staged from the existing verified external P3 builds for this no-bundle POC. They are not committed to the repository and this step is not installer/package validation.
- GPU execution improves throughput and makes larger models practical; it does not make the selected Tiny/Base model intrinsically more accurate. This task does not add the Small/Medium/Turbo/Large choices used by Handy.
- Completion-path automation uses a deterministic loopback provider. P5-011 verified the live privacy-filtered catalog with the configured credential without sending transcript text, audio, or a completion request; real answer quality and latency still require the tester's own bounded POC questions.
