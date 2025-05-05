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
		curve.value(t)
    });
    points.plot(title, Rect(100, 200, 500, 300));
};

// Function to play a sound with time warping applied
~playTimeWarped = {|curve, bufnum, dur=2.0|
    {
        // Start a playback using the time warp curve to modulate position
        var sig;
        var phase;
        var buf_frames;
		var warpTable;
		var warped_phase;

        // Get buffer information
        buf_frames = BufFrames.ir(bufnum);

        // Create a normalized phase from 0 to 1 over the duration at audio rate
        phase = Line.ar(0, 1, dur);

        // For each sample, lookup what the warped phase would be for that position
        // We create a mapping table with a pre-calculated curve that will be interpolated
        warpTable = LocalBuf.newFrom(
            // Create 1024 points of our warping curve for lookup
            Array.fill(1024, {|i|
                var t = i/1023;
                curve.value(t).clip(0, 0.999); // Prevent reading past buffer end
            })
        );

        // Look up the warped phase from our table
        warped_phase = BufRd.ar(1, warpTable, phase * 1023, 1, 1);

        // Use the warped phase to read from the buffer
        sig = BufRd.ar(2, bufnum, warped_phase * buf_frames, 1, 1);

        // Apply an envelope to avoid clicks
        sig = sig * EnvGen.kr(Env.sine(dur), doneAction: 2);

        Out.ar(0, sig);
    }.play;
};

// Record a short sample for testing time warping
~recordTestBuffer = {|dur=4.0|
    var buffer;

    buffer = Buffer.alloc(s, s.sampleRate * dur, 2);

    SynthDef(\recordBuf, {|bufnum|
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

    "Recording test buffer...".postln;
    dur.wait;
    "Recording complete.".postln;

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

// Record a test buffer
(
s.waitForBoot {
    fork {
        ~testBuffer = ~recordTestBuffer.value(4.0);
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
// INTERACTIVE TESTING
// ==============================

// GUI for testing time warping
(
{
var win;
var warpTypeMenu;
var factorSlider;
var directionMenu;
var timeWarpSlider;
var randomizeCheck;
var plotButton;
var audioButton;
var stopButton;
var recordButton;
var currentCurve;
var plotView;
var params;
var testBuffer;
var dirIdx;

// Create window
win = Window("Time Warp Tester", Rect(100, 100, 600, 500)).front;

// GUI elements
warpTypeMenu = PopUpMenu(win, Rect(10, 10, 150, 20))
    .items_(["linear", "exponential", "logarithmic", "sinusoidal", "sCurve", "randomFluctuations", "power"])
    .action_({ |menu|
        // Safely get the direction index
        dirIdx = if(directionMenu.items.notNil && directionMenu.value.notNil, {
            directionMenu.items[directionMenu.value];
        }, {
            "forward"; // Default to forward if not available
        });

        // Handle potential nil values from array lookups
        dirIdx = if(dirIdx.isNil, { "forward" }, { dirIdx });

        params = (
            type: menu.items[menu.value].asSymbol,
            factor: factorSlider.value * 5,
            direction: if(dirIdx == "forward", { 1 }, { if(dirIdx == "reverse", { -1 }, { 0 }) }), // Safer direction handling
            timeWarp: timeWarpSlider.value * 2,
            randomize: randomizeCheck.value
        );

        if(params.type == \power, {
            currentCurve = {|t| ~powerFunctionWarping.value(t, params.factor - 2.5, params.timeWarp, params.randomize) };
        }, {
            currentCurve = ~parametricTimeWarp.(params.type, params.factor, params.direction);
        });
    });

StaticText(win, Rect(10, 40, 150, 20)).string_("Factor / Warp Curve:");
factorSlider = Slider(win, Rect(10, 60, 150, 20))
    .value_(0.5)
    .action_({ |slider|
        // Safely get the direction index
        dirIdx = if(directionMenu.items.notNil && directionMenu.value.notNil, {
            directionMenu.items[directionMenu.value];
        }, {
            "forward"; // Default to forward if not available
        });

        // Handle potential nil values from array lookups
        dirIdx = if(dirIdx.isNil, { "forward" }, { dirIdx });

        params = (
            type: warpTypeMenu.items[warpTypeMenu.value].asSymbol,
            factor: slider.value * 5,
            direction: if(dirIdx == "forward", { 1 }, { if(dirIdx == "reverse", { -1 }, { 0 }) }), // Safer direction handling
            timeWarp: timeWarpSlider.value * 2,
            randomize: randomizeCheck.value
        );

        if(params.type == \power, {
            currentCurve = {|t| ~powerFunctionWarping.value(t, params.factor - 2.5, params.timeWarp, params.randomize) };
        }, {
            currentCurve = ~parametricTimeWarp.(params.type, params.factor, params.direction);
        });
    });

StaticText(win, Rect(10, 90, 150, 20)).string_("Direction:");
directionMenu = PopUpMenu(win, Rect(10, 110, 150, 20))
    .items_(["forward", "reverse", "bidirectional"])
    .action_({ |menu|
        params = (
            type: warpTypeMenu.items[warpTypeMenu.value].asSymbol,
            factor: factorSlider.value * 5,
            direction: if(menu.value == 0, { 1 }, { if(menu.value == 1, { -1 }, { 0 }) }), // Use direct values instead of indexOf
            timeWarp: timeWarpSlider.value * 2,
            randomize: randomizeCheck.value
        );

        if(params.type == \power, {
            currentCurve = {|t| ~powerFunctionWarping.value(t, params.factor - 2.5, params.timeWarp, params.randomize) };
        }, {
            currentCurve = ~parametricTimeWarp.(params.type, params.factor, params.direction);
        });
    });

StaticText(win, Rect(10, 140, 150, 20)).string_("Time Warp Speed:");
timeWarpSlider = Slider(win, Rect(10, 160, 150, 20))
    .value_(0.5)
    .action_({ |slider|
        // Safely get the direction index
        dirIdx = if(directionMenu.items.notNil && directionMenu.value.notNil, {
            directionMenu.items[directionMenu.value];
        }, {
            "forward"; // Default to forward if not available
        });

        // Handle potential nil values from array lookups
        dirIdx = if(dirIdx.isNil, { "forward" }, { dirIdx });

        params = (
            type: warpTypeMenu.items[warpTypeMenu.value].asSymbol,
            factor: factorSlider.value * 5,
            direction: if(dirIdx == "forward", { 1 }, { if(dirIdx == "reverse", { -1 }, { 0 }) }), // Safer direction handling
            timeWarp: slider.value * 2,
            randomize: randomizeCheck.value
        );

        if(params.type == \power, {
            currentCurve = {|t| ~powerFunctionWarping.value(t, params.factor - 2.5, params.timeWarp, params.randomize) };
        }, {
            currentCurve = ~parametricTimeWarp.(params.type, params.factor, params.direction);
        });
    });

randomizeCheck = CheckBox(win, Rect(10, 190, 150, 20))
    .string_("Randomize ±20%")
    .value_(true)
    .action_({ |checkbox|
        // Safely get the direction index
        dirIdx = if(directionMenu.items.notNil && directionMenu.value.notNil, {
            directionMenu.items[directionMenu.value];
        }, {
            "forward"; // Default to forward if not available
        });

        // Handle potential nil values from array lookups
        dirIdx = if(dirIdx.isNil, { "forward" }, { dirIdx });

        params = (
            type: warpTypeMenu.items[warpTypeMenu.value].asSymbol,
            factor: factorSlider.value * 5,
            direction: if(dirIdx == "forward", { 1 }, { if(dirIdx == "reverse", { -1 }, { 0 }) }), // Safer direction handling
            timeWarp: timeWarpSlider.value * 2,
            randomize: checkbox.value
        );

        if(params.type == \power, {
            currentCurve = {|t| ~powerFunctionWarping.value(t, params.factor - 2.5, params.timeWarp, params.randomize) };
        }, {
            currentCurve = ~parametricTimeWarp.(params.type, params.factor, params.direction);
        });
    });

plotButton = Button(win, Rect(10, 220, 150, 30))
    .states_([["Plot Curve"]])
    .action_({
        ~plotTimeWarpCurve.(currentCurve, 100, "Time Warp: " + params.type);
    });

recordButton = Button(win, Rect(10, 260, 150, 30))
    .states_([["Record Test Buffer"]])
    .action_({
        s.waitForBoot {
            fork {
                testBuffer = ~recordTestBuffer.value(4.0);
            }
        }
    });

audioButton = Button(win, Rect(10, 300, 70, 30))
    .states_([["Play"]])
    .action_({
        if(testBuffer.notNil, {
            ~playTimeWarped.(currentCurve, testBuffer, 4.0);
        }, {
            "Please record a test buffer first.".postln;
        });
    });

stopButton = Button(win, Rect(90, 300, 70, 30))
    .states_([["Stop"]])
    .action_({
        s.freeAll;
    });

// Setup default values and initial curve
params = (
    type: \power,
    factor: 2.5,
    direction: 1,
    timeWarp: 1.0,
    randomize: true
);

warpTypeMenu.valueAction = 6;  // Set to power function by default
directionMenu.valueAction = 0;  // Set to forward by default

// Use existing test buffer if available
testBuffer = ~testBuffer;

// Add explanatory text
TextView(win, Rect(170, 10, 420, 320))
    .string_(
        "Time Warping in Simply Droplets\n\n" ++
        "This prototype demonstrates time manipulation within droplets.\n\n" ++
        "The power function warping approach works as follows:\n" ++
        "- Warp Curve > 0: Exponential acceleration effect\n" ++
        "- Warp Curve = 0: Linear playback (no warping)\n" ++
        "- Warp Curve < 0: Logarithmic deceleration effect\n\n" ++
        "The randomization option adds ±20% variation to create more\n" ++
        "organic sounding results. This helps avoid mechanical artifacts\n" ++
        "often associated with granular synthesis.\n\n" ++
        "Usage:\n" ++
        "1. Record a test buffer\n" ++
        "2. Adjust the parameters\n" ++
        "3. Plot the resulting curve\n" ++
        "4. Play to hear the effect\n\n" ++
        "When integrating with droplets, each droplet will have its own\n" ++
        "time warping characteristics, creating a rich, evolving sound."
    )
    .editable_(false);
}.value
);
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
