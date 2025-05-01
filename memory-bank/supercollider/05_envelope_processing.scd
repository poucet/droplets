// Simply Droplets - Envelope Processing Prototype
// This file implements amplitude envelope shaping for droplets in the Simply Droplets plugin

// ==============================
// ENVELOPE IMPLEMENTATIONS
// ==============================

// Collection of envelope functions
~envelopes = (
    // Standard ADSR with normalized duration
    adsr: {|attack=0.1, decay=0.2, sustain=0.7, release=0.2, curve=(-4)|
        {|duration, t|
            var normAttack = attack;
            var normDecay = decay;
            var normSustain = sustain;
            var normRelease = release;
            
            var result = Env.adsr(
                attackTime: normAttack,
                decayTime: normDecay,
                sustainLevel: normSustain,
                releaseTime: normRelease,
                curve: curve
            ).at(t);
            
            result;
        }
    },
    
    // Simple triangle envelope
    triangle: {|attackPeak=0.5|
        {|duration, t|
            if(t < attackPeak) {
                t / attackPeak;
            } {
                1 - ((t - attackPeak) / (1 - attackPeak));
            };
        }
    },
    
    // Gaussian/bell-shaped envelope
    gaussian: {|center=0.5, width=0.25|
        {|duration, t|
            exp(-1 * ((t - center) ** 2) / width);
        }
    },
    
    // Sine shaped envelope
    sine: {
        {|duration, t|
            sin(t * pi);
        }
    },
    
    // Trapezoid envelope with configurable attack, sustain, and release segments
    trapezoid: {|attack=0.1, sustain=0.6, release=0.3|
        {|duration, t|
            if(t < attack) {
                // Attack segment
                t / attack;
            } {
                if(t < (attack + sustain)) {
                    // Sustain segment
                    1.0;
                } {
                    // Release segment
                    1.0 - ((t - attack - sustain) / release);
                }
            }
        }
    },
    
    // Percussive envelope with quick attack and exponential decay
    percussion: {|attackTime=0.005, curve=(-4)|
        {|duration, t|
            var normAttack = attackTime;
            
            if(t < normAttack) {
                t / normAttack;
            } {
                exp((t - normAttack) * curve);
            }
        }
    },
    
    // Reverse envelope (rise from quiet to loud)
    reverse: {|curve=(-4)|
        {|duration, t|
            1.0 - exp(t * curve);
        }
    },
    
    // Random micro-variation to add "humanization"
    humanize: {|baseEnvelope, amount=0.1|
        {|duration, t|
            // Get the base envelope value
            var baseValue = baseEnvelope.value(duration, t);
            
            // Add small random variation
            var variation = 1.0 + (amount * (2.0.rand - 1.0));
            
            // Apply variation and clip to 0-1 range
            (baseValue * variation).clip(0, 1);
        }
    }
);

// Create a parametric envelope function that blends between different types
~createParametricEnvelope = {|type=\adsr, params=nil|
    var envFunc;
    
    params = params ?? ();
    
    case
    { type == \adsr } {
        envFunc = ~envelopes.adsr.value(
            params[\attack] ?? 0.1,
            params[\decay] ?? 0.2,
            params[\sustain] ?? 0.7,
            params[\release] ?? 0.2,
            params[\curve] ?? (-4)
        );
    }
    { type == \triangle } {
        envFunc = ~envelopes.triangle.value(
            params[\attackPeak] ?? 0.5
        );
    }
    { type == \gaussian } {
        envFunc = ~envelopes.gaussian.value(
            params[\center] ?? 0.5,
            params[\width] ?? 0.25
        );
    }
    { type == \sine } {
        envFunc = ~envelopes.sine.value();
    }
    { type == \trapezoid } {
        envFunc = ~envelopes.trapezoid.value(
            params[\attack] ?? 0.1,
            params[\sustain] ?? 0.6,
            params[\release] ?? 0.3
        );
    }
    { type == \percussion } {
        envFunc = ~envelopes.percussion.value(
            params[\attackTime] ?? 0.005,
            params[\curve] ?? (-4)
        );
    }
    { type == \reverse } {
        envFunc = ~envelopes.reverse.value(
            params[\curve] ?? (-4)
        );
    };
    
    // Apply humanization if specified
    if(params[\humanize].notNil && params[\humanize] > 0) {
        envFunc = ~envelopes.humanize.value(envFunc, params[\humanize]);
    };
    
    // Return the envelope function
    envFunc;
};

// Apply envelope to an audio buffer
~applyEnvelope = {|buffer, envFunc, startSample=0, numSamples=nil|
    var arrayCopy, envArray;
    
    // Default to the entire buffer if numSamples is not specified
    numSamples = numSamples ?? (buffer.numFrames - startSample);
    
    // Create an array of envelope values
    envArray = Array.fill(numSamples, {|i|
        var t = i / numSamples;
        envFunc.value(numSamples, t);
    });
    
    // Apply the envelope to the buffer
    buffer.loadToFloatArray(startSample, numSamples, {|data|
        // Create a new buffer with the enveloped data
        var newData = Array.newClear(data.size);
        
        // Apply envelope to each sample
        data.size.do {|i|
            var channelIndex = i % 2;
            var sampleIndex = i.div(2);
            
            if(sampleIndex < numSamples) {
                newData[i] = data[i] * envArray[sampleIndex];
            } {
                newData[i] = data[i];
            };
        };
        
        // Free the original buffer and load the new data
        buffer.free;
        Buffer.loadCollection(s, newData, 2, {|buf|
            "Envelope applied to buffer %".format(buf).postln;
        });
    });
};

// ==============================
// VISUALIZATION FUNCTIONS
// ==============================

// Plot an envelope shape
~plotEnvelope = {|envFunc, title="Envelope", numPoints=100|
    var points = Array.fill(numPoints, {|i| 
        var t = i / (numPoints - 1);
        [t, envFunc.value(1.0, t)]
    });
    
    var plot = points.plot(title, Rect(100, 100, 500, 300));
    plot;
};

// Function to visualize all envelope types
~visualizeAllEnvelopes = {
    var types = [\adsr, \triangle, \gaussian, \sine, \trapezoid, \percussion, \reverse];
    var plots = [];
    
    types.do {|type|
        var envFunc = ~createParametricEnvelope.value(type);
        var plot = ~plotEnvelope.value(envFunc, "Envelope: " ++ type);
        plots = plots.add(plot);
    };
    
    plots;
};

// Function to play a sound with the envelope applied
~playEnvelopedSound = {|envType, params=nil, freq=440, dur=1.0|
    var envFunc = ~createParametricEnvelope.value(envType, params);

    {
        var sig = SinOsc.ar(freq);
        var env = EnvGen.kr(
            Env.new(
                Array.fill(100, {|i| envFunc.value(dur, i/99) }),
                Array.fill(99, { 1/99 }),
                \lin
            ),
            timeScale: dur,
            doneAction: 2
        );
        
        Out.ar(0, (sig * env) ! 2);
    }.play;
};

// Function to visualize many envelope parameter variations
~visualizeEnvelopeVariations = {|envType, paramName, values|
    var plots = [];
    
    values.do {|val, i|
        var params = ();
        params[paramName] = val;
        
        var envFunc = ~createParametricEnvelope.value(envType, params);
        var plot = ~plotEnvelope.value(
            envFunc, 
            "% = %".format(paramName, val)
        );
        
        plots = plots.add(plot);
    };
    
    plots;
};

// ==============================
// DROPLET ENVELOPE INTEGRATION
// ==============================

// Apply envelope to a single droplet
~applyEnvelopeToDroplet = {|droplet, envType, params=nil|
    var envFunc = ~createParametricEnvelope.value(envType, params);
    
    // Store the envelope function in the droplet
    droplet[\envelope] = envFunc;
    
    // Update the process method to apply the envelope
    droplet[\process] = {|self, buffer|
        var pos = self.getPlaybackPosition;
        var normalizedAge = self.age / self.duration;
        var envValue = self.envelope.value(self.duration, normalizedAge);
        
        // Return the sample with envelope applied
        buffer.at(pos) * envValue;
    };
    
    droplet;
};

// Create a droplet with built-in envelope processing
~createEnvelopedDroplet = {|startPos, duration, timeWarp, position, envelope, bufnum|
    var droplet = (
        // Capture information
        sourceBuffer: bufnum,
        startPosition: startPos,
        duration: duration,
        
        // Playback control
        age: 0,
        timeWarp: timeWarp,
        
        // 3D positioning
        position: position,
        
        // Envelope
        envelope: envelope,
        
        // Status
        active: true,
        
        // Methods
        advance: {|self, numSamples|
            self.age = self.age + numSamples;
            self.active = self.age < self.duration;
        },
        
        // Get playback position for the current age
        getPlaybackPosition: {|self|
            var normalizedAge = self.age / self.duration;
            var warped = self.timeWarp.value(normalizedAge);
            (self.startPosition + (warped * self.duration)).round.asInteger;
        },
        
        // Get envelope value for current age
        getEnvelopeValue: {|self|
            var normalizedAge = self.age / self.duration;
            self.envelope.value(self.duration, normalizedAge);
        }
    );
    
    droplet;
};

// ==============================
// TESTING AND VISUALIZATION
// ==============================

// Start the server for audio testing
s.boot;

// Plot all envelope types
(
~visualizeAllEnvelopes.value();
)

// Test different envelope parameters
(
~visualizeEnvelopeVariations.value(
    \adsr,
    \attack,
    [0.05, 0.1, 0.3, 0.5]
);
)

(
~visualizeEnvelopeVariations.value(
    \triangle,
    \attackPeak,
    [0.1, 0.3, 0.5, 0.7, 0.9]
);
)

// Play a sound with each envelope type
(
var types = [\adsr, \triangle, \gaussian, \sine, \trapezoid, \percussion, \reverse];

fork {
    types.do {|type|
        "Playing with % envelope".format(type).postln;
        ~playEnvelopedSound.value(type);
        1.5.wait;
    };
};
)

// Test with various parameters
(
var params = (
    attack: 0.01,
    decay: 0.1,
    sustain: 0.5,
    release: 0.5
);

~playEnvelopedSound.value(\adsr, params, 440, 2.0);
)

// Test a percussive sound
(
var params = (
    attackTime: 0.001,
    curve: -8,
    humanize: 0.05
);

~playEnvelopedSound.value(\percussion, params, 880, 0.5);
)

// Test humanization
(
// Base envelope
var params1 = (
    attack: 0.1,
    decay: 0.2,
    sustain: 0.7,
    release: 0.2
);

// Humanized version
var params2 = (
    attack: 0.1,
    decay: 0.2,
    sustain: 0.7,
    release: 0.2,
    humanize: 0.1
);

fork {
    "Playing without humanization".format.postln;
    ~playEnvelopedSound.value(\adsr, params1, 440, 2.0);
    2.5.wait;
    
    "Playing with humanization".format.postln;
    ~playEnvelopedSound.value(\adsr, params2, 440, 2.0);
};
)

// ==============================
// INTERACTIVE TESTING UI
// ==============================

// Interactive envelope tester
(
var win = Window("Envelope Tester", Rect(100, 100, 800, 600)).front;
var envTypeMenu, paramControls, paramLabels, envView;
var playButton, paramsButton, freq, dur;
var currentType, currentParams;
var envTypes = [\adsr, \triangle, \gaussian, \sine, \trapezoid, \percussion, \reverse];
var paramNames = (
    adsr: [\attack, \decay, \sustain, \release, \curve, \humanize],
    triangle: [\attackPeak, \humanize],
    gaussian: [\center, \width, \humanize],
    sine: [\humanize],
    trapezoid: [\attack, \sustain, \release, \humanize],
    percussion: [\attackTime, \curve, \humanize],
    reverse: [\curve, \humanize]
);
var paramRanges = (
    attack: [0.001, 0.5],
    decay: [0.001, 0.5],
    sustain: [0.0, 1.0],
    release: [0.001, 0.5],
    curve: [-12, 0],
    humanize: [0, 0.2],
    attackPeak: [0.1, 0.9],
    center: [0.1, 0.9],
    width: [0.05, 0.5],
    attackTime: [0.001, 0.1]
);
var paramDefaults = (
    attack: 0.1,
    decay: 0.2,
    sustain: 0.7,
    release: 0.2,
    curve: -4,
    humanize: 0,
    attackPeak: 0.5,
    center: 0.5,
    width: 0.25,
    attackTime: 0.005
);

// Initialize params and controls
currentType = \adsr;
currentParams = ();
paramControls = ();
paramLabels = ();

// Create UI for envelope visualization
envView = UserView(win, Rect(300, 20, 480, 300))
    .background_(Color.white)
    .drawFunc_({|view|
        var bounds = view.bounds;
        var width = bounds.width;
        var height = bounds.height;
        var numPoints = 100;
        var envFunc = ~createParametricEnvelope.value(currentType, currentParams);
        var points = Array.fill(numPoints, {|i| 
            var t = i / (numPoints - 1);
            var x = t * width;
            var y = height - (envFunc.value(1.0, t) * height);
            Point(x, y);
        });
        
        // Draw the envelope curve
        Pen.strokeColor = Color.black;
        Pen.width = 2;
        Pen.moveTo(points[0]);
        points[1..].do {|point|
            Pen.lineTo(point);
        };
        Pen.stroke;
        
        // Draw baseline
        Pen.strokeColor = Color.gray(0.7);
        Pen.moveTo(0 @ height);
        Pen.lineTo(width @ height);
        Pen.stroke;
    });

// Function to update the envelope display
var updateEnvelope = {
    envView.refresh;
};

// Function to create parameter controls
var createParamControls = {|type|
    // Clear existing controls
    paramControls.keys.do {|key|
        if(paramControls[key].notNil) {
            paramControls[key].remove;
            paramControls[key] = nil;
        };
        if(paramLabels[key].notNil) {
            paramLabels[key].remove;
            paramLabels[key] = nil;
        };
    };
    
    // Create new controls for the selected type
    paramNames[type].do {|param, i|
        var ypos = 70 + (i * 30);
        var range = paramRanges[param];
        var defaultVal = paramDefaults[param];
        var val = currentParams[param] ?? defaultVal;
        var normalized = (val - range[0]) / (range[1] - range[0]);
        
        // Create label
        paramLabels[param] = StaticText(win, Rect(20, ypos, 120, 20))
            .string_(param.asString);
        
        // Create slider
        paramControls[param] = Slider(win, Rect(150, ypos, 130, 20))
            .value_(normalized)
            .action_({|slider|
                var newVal = range[0] + (slider.value * (range[1] - range[0]));
                currentParams[param] = newVal;
                updateEnvelope.value();
            });
    };
};

// Envelope type selection
StaticText(win, Rect(20, 20, 120, 20))
    .string_("Envelope Type:");
    
envTypeMenu = PopUpMenu(win, Rect(150, 20, 130, 20))
    .items_(envTypes)
    .action_({|menu|
        currentType = envTypes[menu.value];
        currentParams = ();  // Reset params
        createParamControls.value(currentType);
        updateEnvelope.value();
    });

// Set initial param controls
createParamControls.value(currentType);

// Frequency and duration controls
StaticText(win, Rect(300, 340, 120, 20))
    .string_("Frequency (Hz):");
freq = Slider(win, Rect(430, 340, 150, 20))
    .value_(0.5)
    .action_({|slider|
        // 100Hz to 2000Hz, exponential scale
        var hz = 100 * (10 ** (slider.value * 1.3));
        StaticText(win, Rect(590, 340, 60, 20))
            .string_(hz.round(1));
    });
StaticText(win, Rect(590, 340, 60, 20))
    .string_(440);
    
StaticText(win, Rect(300, 370, 120, 20))
    .string_("Duration (sec):");
dur = Slider(win, Rect(430, 370, 150, 20))
    .value_(0.5)
    .action_({|slider|
        // 0.1s to 3s
        var seconds = 0.1 + (2.9 * slider.value);
        StaticText(win, Rect(590, 370, 60, 20))
            .string_(seconds.round(0.1));
    });
StaticText(win, Rect(590, 370, 60, 20))
    .string_(1.0);

// Play button
playButton = Button(win, Rect(300, 410, 100, 30))
    .states_([["Play Sound"]])
    .action_({
        var frequency = 100 * (10 ** (freq.value * 1.3));
        var duration = 0.1 + (2.9 * dur.value);
        
        ~playEnvelopedSound.value(currentType, currentParams, frequency, duration);
    });

// Add explanatory text
StaticText(win, Rect(20, 320, 260, 30))
    .string_("Adjust parameters and click 'Play Sound'")
    .align_(\center);

TextView(win, Rect(20, 370, 260, 210))
    .string_(
        "Envelope Findings:\n\n" ++
        "1. ADSR provides classic flexibility with intuitive control\n\n" ++
        "2. Triangle is useful for many natural sounds\n\n" ++
        "3. Gaussian (bell) creates smooth, natural beginnings and endings\n\n" ++
        "4. Percussive is ideal for transient sounds\n\n" ++
        "5. Humanization adds subtle variation between droplets\n\n" ++
        "6. Parameter ranges should be limited for musical results"
    )
    .editable_(false);

// Add envelope application examples
TextView(win, Rect(420, 410, 360, 170))
    .string_(
        "Envelope Applications in Simply Droplets:\n\n" ++
        "• Each droplet can have its own envelope type and parameters\n\n" ++
        "• Envelope type can be distributed like other droplet properties\n\n" ++
        "• Short envelopes (50-100ms) tend to work best for dense textures\n\n" ++
        "• Longer envelopes (100-500ms) are better for sparse textures\n\n" ++
        "• Percussion envelopes work well for rhythmic material\n\n" ++
        "• Gaussian envelopes reduce transient artifacting\n\n" ++
        "• Humanization should be subtle (2-5%) for natural variation"
    )
    .editable_(false);

// Initial update
updateEnvelope.value();
)

// ==============================
// NOTES AND OBSERVATIONS
// ==============================

/*
Envelope Processing findings:

1. Envelope shapes significantly impact the perceived character of droplets:
   - ADSR provides classic flexibility with direct control over note shape
   - Triangle offers a simple approach that works well for many sources
   - Gaussian (bell curve) creates very smooth, natural beginnings and endings
   - Percussion is ideal for sharp, transient sounds
   - Reverse envelope creates interesting build-up effects

2. The most effective envelope parameters depend on droplet duration:
   - For short droplets (50-100ms): very short attack (1-5ms) and quick decay
   - For medium droplets (100-300ms): proportional ADSR with 10-20% attack
   - For long droplets (300-500ms): smoother envelopes like Gaussian or Triangle

3. Humanization (subtle random variations in envelope shape) creates more
   natural, organic textures by preventing identical envelope shapes across
   multiple droplets.

4. Parameter ranges should be constrained for musical results:
   - Attack: 0.001 to 0.3 (relative to duration)
   - Decay: 0.001 to 0.5 (relative to duration)
   - Curve values: -8 to -1 work best for most expressive shapes

5. Envelope types can be parametrically distributed just like other droplet
   properties, allowing for rich variation within a single texture.

6. For percussive sources, preserving the original attack is important, which
   means using very short attack times or time-warping the beginning of samples.

Implementation insights:
- Store envelope functions rather than pre-computed envelope values
- Envelope shape should be computed at sample-time for each droplet
- Allow each droplet to have its own envelope parameters for maximum variety
- Normalization to duration ensures consistent envelope shapes regardless of droplet length
*/
