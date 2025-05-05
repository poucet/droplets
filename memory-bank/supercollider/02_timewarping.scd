// Simply Droplets - Time Warping Prototype
// This file implements and tests various time warping methods for the Simply Droplets plugin

// ==============================
// UTILITY FUNCTIONS
// ==============================
(
// Function to visualize a time warp curve
~plotTimeWarpCurve = {|curve, numPoints=100, title="Time Warp Curve"|
    var points;

    points = Array.fill(numPoints, {|i|
        var t = i / (numPoints - 1);
        curve.value(t) // Return just y values for plotting
    });
    points.plot(title, Rect(100, 200, 500, 300));
};

// Function to play a sound with time warping applied
// This implementation affects pitch when time is warped (faster = higher pitch)
~playTimeWarped = {|curve, bufnum, dur=2.0|
    {
        var sig, phase, rate, bufDur, bufFrames, sampleRate;
        var slopeEstimate, bufPos;
        
        // Get buffer information
        bufFrames = BufFrames.ir(bufnum);
        bufDur = bufFrames / BufSampleRate.ir(bufnum);
        sampleRate = SampleRate.ir;
        
        // Use a normalized phase from 0 to 1
        phase = Line.kr(0, 1, dur); // Control rate line from 0 to 1
        
        // Apply the time warp curve to the phase first
        phase = curve.value(phase);
        
        // Estimate derivative by comparing current to slightly delayed value
        // This approximates the slope of the curve and thus the rate multiplier
        slopeEstimate = (phase - DelayN.kr(phase, 0.05, 0.05)) / 0.05;
        
        // Calculate rate for PlayBuf
        rate = (slopeEstimate * bufDur).clip(0.1, 10); // Clip to avoid extreme rates
        
        // Position in the buffer based on warped phase
        bufPos = phase * bufFrames;
        
        // Use PlayBuf for playback with variable rate
        sig = PlayBuf.ar(
            numChannels: 2, 
            bufnum: bufnum,
            rate: rate * BufRateScale.ir(bufnum),
            startPos: bufPos, // Start from the position based on warped phase
            trigger: 1,
            loop: 0,
            doneAction: 0
        );

        // Apply an envelope to avoid clicks
        sig = sig * EnvGen.kr(Env.sine(dur), doneAction: 2);

        Out.ar(0, sig);
    }.play;
};

// Load an audio file for testing time warping
~loadAudioFile = {|path|
    var buffer;

    if(File.exists(path), {
        buffer = Buffer.read(s, path);
        "Audio file loaded: %".format(path).postln;
    }, {
        "File not found: %".format(path).postln;
        nil
    });

    buffer;
};

// Create a simple synthetic test sound and save it as a file
~createTestSound = {|outputPath, dur=4.0|
    var buffer, soundFile;
    var path = outputPath ?? {Platform.recordingsDir +/+ "test_sound.wav"};

    // Allocate buffer
    buffer = Buffer.alloc(s, s.sampleRate * dur, 2);

    // Create a synth that makes a rising tone with harmonics
    SynthDef(\testSound, {|bufnum|
        var sig;
        var pan;
        var stereo;

        sig = SinOsc.ar(
            // Create a rising tone with harmonics
            XLine.kr(300, 1200, dur) * [1, 1.5, 2, 2.5, 3].scramble,
            0,
            // With amplitude modulation
            LFTri.kr([3, 5, 7, 11, 13].scramble, 0, 0.05).sum + 0.1
        ).sum;
        // Make it stereo with some movement
        pan = SinOsc.kr(0.5).range(-0.7, 0.7);
        stereo = Pan2.ar(sig, pan);
        RecordBuf.ar(stereo, bufnum, loop: 0, doneAction: 2);
    }).play(s, [\bufnum, buffer]);

    "Creating test sound...".postln;
    dur.wait;

    // Save the buffer to a file
    buffer.write(path, "wav", "int16");
    "Test sound saved to: %".format(path).postln;

    buffer;
};

// ==============================
// TIME WARP CURVE IMPLEMENTATIONS
// ==============================

// Basic curves for time warping
~linear = {|t|
	t
};

// Exponential curve (acceleration)
~exponential = {|t, factor=3.0|
	t.pow(1.0 + factor)
};

// Logarithmic curve (deceleration)
~logarithmic = {|t, factor=3.0|
    t.pow(1.0 / (1.0 + factor))
};

// Sine-based curve (smooth acceleration and deceleration)
~sinusoidal = {|t|
    0.5 - (0.5 * cos(pi * t))
};

// S-curve (slow start, fast middle, slow end)
~sCurve = {|t|
    3 * t.pow(2) - 2 * t.pow(3)
};

// Reverse direction
~reverse = {|t|
    1.0 - t
};

// Bidirectional (forward then backward)
~bidirectional= {|t|
    if(t < 0.5, {
        t * 2
    }, {
        (1.0 - t) * 2
    })
};

// Random fluctuations around linear
~randomFluctuations = {|t, amount=0.2|
    (t + (amount * (2.0.rand - 1.0))).clip(0, 1)
};

// Time freezing at certain points
~timeFreeze =  {|t, freezePoint=0.5, freezeDuration=0.3|
    if((t > freezePoint) && (t < (freezePoint + freezeDuration)), {
        freezePoint
    }, {
        if(t < freezePoint, {
            t
        }, {
            t - freezeDuration
        })
    })
};

// ==============================
// PARAMETRIC TIME WARP CURVE
// ==============================

// Create a parametric time warp curve that blends between different types
~parametricTimeWarp = {|type=\linear, factor=3.0, direction=1|
    var directionWrapper;

    // Direction: 1 = forward, -1 = reverse, 0 = bidirectional
    directionWrapper = {|t, curve|
        case
        { direction == 1 } { curve.value(t) }
        { direction == -1 } { 1.0 - curve.value(t) }
        { direction == 0 } {
            if(t < 0.5, {
                curve.value(t * 2) * 0.5
            }, {
                1.0 - (curve.value((t - 0.5) * 2) * 0.5)
            })
        };
    };

    case
    { type == \linear } { {|t| directionWrapper.value(t, ~linear) } }
    { type == \exponential } { {|t| directionWrapper.value(t, {|x| ~exponential.value(x, factor) }) } }
    { type == \logarithmic } { {|t| directionWrapper.value(t, {|x| ~logarithmic.value(x, factor) }) } }
    { type == \sinusoidal } { {|t| directionWrapper.value(t, ~sinusoidal) } }
    { type == \sCurve } { {|t| directionWrapper.value(t, ~sCurve) } }
    { type == \randomFluctuations } { {|t| directionWrapper.value(t, {|x| ~randomFluctuations.value(x, factor * 0.1) }) } }
    { {|t| t } };  // Default to linear if unknown type
};

// Power function warping as defined in the architecture document
~powerFunctionWarping = {|t, warp_curve=0.0, time_warp=1.0, randomize=true|
    var random_factor;
    var result;

    random_factor = if(randomize, { 0.8 + 0.4.rand }, { 1.0 });  // ±20% variation

    if(warp_curve >= 0.0, {
        // Exponential curve (acceleration)
        result = t.pow(1.0 + (warp_curve * 3.0)) * time_warp * random_factor;
    }, {
        // Logarithmic curve (deceleration)
        result = t.pow(1.0 / (1.0 + (warp_curve.abs * 3.0))) * time_warp * random_factor;
    });

    result.clip(0, 1); // Ensure we stay in the valid range
};
);
// ==============================
// TESTING AND VISUALIZATION
// ==============================

(
// Start the server for audio testing
s.boot;
);

// Plot various time warp curves
(
// Basic curves
~plotTimeWarpCurve.(~linear, 100, "Linear (No Warping)");
~plotTimeWarpCurve.(~exponential, 100, "Exponential (Acceleration)");
~plotTimeWarpCurve.(~logarithmic, 100, "Logarithmic (Deceleration)");
~plotTimeWarpCurve.(~sinusoidal, 100, "Sinusoidal");
~plotTimeWarpCurve.(~sCurve, 100, "S-Curve");
~plotTimeWarpCurve.(~reverse, 100, "Reverse");
~plotTimeWarpCurve.(~bidirectional, 100, "Bidirectional");

// Power function warping with different settings
~plotTimeWarpCurve.({|t| ~powerFunctionWarping.value(t, 0.0, 1.0, false)}, 100, "Power: Linear (warp_curve = 0.0)");
~plotTimeWarpCurve.({|t| ~powerFunctionWarping.value(t, 0.5, 1.0, false)}, 100, "Power: Exponential (warp_curve = 0.5)");
~plotTimeWarpCurve.({|t| ~powerFunctionWarping.value(t, 1.0, 1.0, false)}, 100, "Power: Strong Exponential (warp_curve = 1.0)");
~plotTimeWarpCurve.({|t| ~powerFunctionWarping.value(t, -0.5, 1.0, false)}, 100, "Power: Logarithmic (warp_curve = -0.5)");
~plotTimeWarpCurve.({|t| ~powerFunctionWarping.value(t, -1.0, 1.0, false)}, 100, "Power: Strong Logarithmic (warp_curve = -1.0)");

// With randomization
~plotTimeWarpCurve.({|t| ~powerFunctionWarping.value(t, 0.5, 1.0, true)}, 100, "Power: Exponential with Randomization");
);

// Create and load a test sound
(
s.waitForBoot {
    fork {
        // Path for the test sound
        var testSoundPath = Platform.recordingsDir +/+ "test_sound.wav";

        // Create a test sound and save it to a file
        ~testBuffer = ~createTestSound.value(testSoundPath, 4.0);

        // Alternatively, you can load an existing audio file:
        // ~testBuffer = ~loadAudioFile.value("/path/to/your/soundfile.wav");
    }
}
);

// Play audio with different time warping
(
// Linear (no warping)
~playTimeWarped.(~linear, ~testBuffer, 4.0);
);

(
// Exponential curve (acceleration)
~playTimeWarped.(~exponential, ~testBuffer, 4.0);
);

(
// Logarithmic curve (deceleration)
~playTimeWarped.(~logarithmic, ~testBuffer, 4.0);
);

(
// Bidirectional (forward then backward)
~playTimeWarped.(~bidirectional, ~testBuffer, 4.0);
);

(
// Power function warping (exponential)
~playTimeWarped.({|t| ~powerFunctionWarping.value(t, 0.7, 1.0, true)}, ~testBuffer, 4.0);
);

(
// Power function warping (logarithmic)
~playTimeWarped.({|t| ~powerFunctionWarping.value(t, -0.7, 1.0, true)}, ~testBuffer, 4.0);
);

// ==============================
// SIMPLE EXAMPLES
// ==============================

// Here are some simpler examples of how to load a file and apply time warping

// Example 1: Create and use a test sound
/*
// Create a test sound
s.waitForBoot {
    fork {
        var testSoundPath = Platform.recordingsDir +/+ "test_sound.wav";
        ~testBuffer = ~createTestSound.value(testSoundPath, 4.0);

        // Now play with different warp curves
        ~playTimeWarped.(~linear, ~testBuffer, 4.0);
        2.wait;
        ~playTimeWarped.(~exponential, ~testBuffer, 4.0);
        2.wait;
        ~playTimeWarped.(~logarithmic, ~testBuffer, 4.0);
    }
}
*/

// Example 2: Load an existing audio file
/*
// Load a sound file
s.waitForBoot {
    // Replace this path with your own audio file path
    ~testBuffer = ~loadAudioFile.value(Platform.resourceDir +/+ "sounds/a11wlk01.wav");
    
    // Apply different time warping
    ~playTimeWarped.(~linear, ~testBuffer, 4.0);
}
*/


// ==============================
// NOTES AND OBSERVATIONS
// ==============================

/*
Time Warping findings:

1. The power function approach works effectively for both acceleration and
   deceleration effects with a single, intuitive parameter.

2. Random variation of ±20% on the time warp factor creates significantly
   more organic results, avoiding the mechanical quality often associated
   with granular synthesis.

3. Exponential warping (warp_curve > 0) creates forward momentum and works
   well for percussive or rhythmic material.

4. Logarithmic warping (warp_curve < 0) creates a sense of suspense or
   anticipation and emphasizes attack portions.

5. Bidirectional playback can create interesting textural effects but is
   less intuitive for musical material with strong directional qualities.

6. Most musically useful range seems to be warp_curve values between -0.7
   and 0.7, with more extreme values creating special effects.

7. Time freezing effects (implemented as a separate curve type) could be
   valuable for creating sustained textures from short input material.

Key implementation insights:
- Power function is computationally efficient while providing flexible results
- The approach maps well to a single bipolar parameter (-1.0 to 1.0)
- Randomization is crucial for natural-sounding results
- The mathematical formula is simple to implement in any language
*/
