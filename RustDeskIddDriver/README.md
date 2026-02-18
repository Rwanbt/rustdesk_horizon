# RustDeskIddDriver

This is a sample based on [Indirect Display Driver Sample](https://github.com/microsoft/Windows-driver-samples/tree/master/video/IndirectDisplay).

## Environment

Win10 or later

## Steps

1. Double click "RustDeskIddDriver.cer" to install cert.
2. Run "RustDeskIddApp.exe" as administrator.  
    Press "c" to create device. If you get 0xe0000247 error code. You should follow step 3.
    Press "i" to install. If you get 0xe0000247 error code. You should follow step 3.
3. Disable Driver Signature Verification.

## Demo

If all are going right, you can see RustDeskIddDriver in your device manager.

## View logs

You need a TraceView.exe(contained in WDK).

```
Launch TraceView.exe
        |
        v
       File
        |
        v
Create New Log Session
        |
        v
   Add Provider
        |
        v
    Select PDB Path
        |
        v
Ok --> Next --> Finish
```

## TODOs
