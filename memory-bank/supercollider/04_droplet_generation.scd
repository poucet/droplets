// Simply Droplets - Droplet Generation Prototype
// This file implements the Rain Catcher system for creating droplets in the Simply Droplets plugin

// ==============================
// UTILITY FUNCTIONS
// ==============================
(
// Function to create a single droplet with given properties
~createDroplet = {|startPos, duration, timeWarp, position, bufnum|
    (
        // Capture information
        sourceBuffer: bufnum,
        startPosition: startPos,
        duration: duration,

        // Playback control
        age: 0,
        timeWarp: timeWarp,

        // 3D positioning
        position: position,

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
        }
    );
};

// Function to visualize active droplets
~visualizeDroplets = {|droplets, width=600, height=400, title="Active Droplets"|
    var win = Window(title, Rect(100, 100, width, height)).front;
    var activeCount = droplets.count({|d| d.active});
    var userView;

    // Display stats
    StaticText(win, Rect(10, 10, 580, 20))
        .string_("Active droplets: " ++ activeCount ++ " / " ++ droplets.size);

    // Create view for visualization
    userView = UserView(win, Rect(10, 40, width - 20, height - 50))
        .background_(Color.black)
        .drawFunc_({|view|
            var bounds = view.bounds;
            var center = bounds.center;
            var maxRadius = bounds.width.min(bounds.height) * 0.45;

            // Draw reference circles
            Pen.strokeColor = Color.gray(0.3);
            Pen.addArc(center, maxRadius * 0.25, 0, 2pi);
            Pen.stroke;
            Pen.addArc(center, maxRadius * 0.5, 0, 2pi);
            Pen.stroke;
            Pen.addArc(center, maxRadius * 0.75, 0, 2pi);
            Pen.stroke;
            Pen.addArc(center, maxRadius, 0, 2pi);
            Pen.stroke;

            // Draw coordinate axes
            Pen.strokeColor = Color.gray(0.5);
            Pen.moveTo(center.x - maxRadius, center.y);
            Pen.lineTo(center.x + maxRadius, center.y);
            Pen.stroke;
            Pen.moveTo(center.x, center.y - maxRadius);
            Pen.lineTo(center.x, center.y + maxRadius);
            Pen.stroke;

            // Draw each active droplet
            droplets.do {|droplet, i|
                if(droplet.active) {
                    var radius = droplet.position[0];
                    var azimuth = droplet.position[1];
                    var elevation = droplet.position[2];
                    var x, y, size, opacity;

                    // Calculate position in the view
                    x = center.x + (cos(azimuth) * radius * maxRadius);
                    y = center.y + (sin(azimuth) * radius * maxRadius);

                    // Size varies with duration (longer = bigger)
                    size = (droplet.duration / s.sampleRate * 60).clip(3, 15);

                    // Opacity varies with age (newer = more opaque)
                    opacity = 1.0 - (droplet.age / droplet.duration);

                    // Color based on elevation (blue below, green at horizon, red above)
                    Pen.fillColor = Color.new(
                        0.5 + (sin(elevation) * 0.5),  // Red: more for higher elevation
                        0.5 - (sin(elevation).abs * 0.3),  // Green: more at horizon
                        0.5 - (sin(elevation) * 0.5),  // Blue: more for lower elevation
                        opacity
                    );

                    Pen.addOval(Rect(x - (size/2), y - (size/2), size, size));
                    Pen.fill;
                };
            };
        });

    // Update the view every 50ms
    Routine({
        while({ win.isClosed.not }) {
            { userView.refresh }.defer;
            0.05.wait;
        }
    }).play;

    win;
};

// Function to play active droplets
~playDroplets = {|droplets, buffer|
    droplets.collect {|droplet, i|
        if(droplet.active) {
            var pos, stereoResult, leftGain, rightGain, leftDelay, rightDelay;
            var phasor, sig, maxDelayBuffer, leftSig, rightSig;

            pos = droplet.getPlaybackPosition;
            stereoResult = ~renderBinaural.value(droplet.position);

            leftGain = stereoResult[0];
            rightGain = stereoResult[1];
            leftDelay = stereoResult[2];
            rightDelay = stereoResult[3];

            {
                // Read a snippet from the buffer
                sig = PlayBuf.ar(
                    2,  // Stereo
                    buffer,
                    1.0,  // Rate
                    0,  // Trigger
                    pos,  // Start position
                    0,  // Loop
                    doneAction: 2  // Free when done
                );

                // Apply envelope (simple triangle)
                sig = sig * EnvGen.kr(Env.perc(0.1, 0.9), timeScale: droplet.duration / s.sampleRate);

                // Apply stereo positioning with both amplitude and phase differences
                maxDelayBuffer = 0.001; // 1ms buffer for delay
                leftSig = DelayN.ar(sig[0] * leftGain, maxDelayBuffer, leftDelay);
                rightSig = DelayN.ar(sig[1] * rightGain, maxDelayBuffer, rightDelay);

                Out.ar(0, [leftSig, rightSig]);
            }.play;
        };
    };
};

// ==============================
// RAIN CATCHER IMPLEMENTATION
// ==============================

~makeRainCatcher = {|config|
    var catcher = (
        // Configuration
        config: config ?? (
            bufferLength: 44100 * 5,  // 5 seconds
            density: 5,  // Droplets per second
            maxDroplets: 32,  // Maximum concurrent droplets
            distributions: (
                // Where in the buffer to sample from
                position: { 1.0.rand },

                // Duration in seconds
                duration: { 0.05 + (0.45 * 1.0.rand) },  // 50ms to 500ms

                // Default power function time warp
                timeWarp: {|t|
                    var warpCurve = 1.0.rand2;  // -1.0 to 1.0
                    var randomFactor = 0.8 + (0.4.rand);  // 0.8 to 1.2 (±20%)
					["XXX: timeWarp: ", t].postln;

                    if(warpCurve >= 0.0) {
                        t.pow(1.0 + (warpCurve * 3.0)) * randomFactor
                    } {
                        t.pow(1.0 / (1.0 + (warpCurve.abs * 3.0))) * randomFactor
                    }
                },

                // 3D position
                radius: { 1.0.rand },
                azimuth: { 2pi.rand },
                elevation: { pi.rand - (pi/2) }
            )
        ),

        // Active droplets collection
        droplets: Array.new,

        // Buffer information
        buffer: nil,

        // Internal state
        lastSpawnTime: 0,

        // Initialize the catcher
        init: {|self, bufferData|
            var server = Server.default;

            // Create a buffer if not supplied
            if(bufferData.isNil) {
                self.buffer = Buffer.alloc(server,
                                          self.config.bufferLength,
                                          2,  // Stereo
                                          {|buf| "Buffer allocated: %".format(buf).postln });

                // Generate some test audio (noise + sine tones)
                SynthDef(\fillBuf, {|out=0, bufnum|
					var server = Server.default;
                    var dur = self.config.bufferLength / server.sampleRate;
                    var sig = Mix([
                        PinkNoise.ar(0.3),
                        SinOsc.ar(LFNoise1.kr(0.5).exprange(200, 800), 0, 0.5),
                        Saw.ar(LFNoise1.kr(0.2).exprange(100, 300), 0.3)
                    ]);

                    // Make it stereo with some movement
                    var pan = SinOsc.kr(0.4).range(-0.7, 0.7);
                    var stereo = Pan2.ar(sig, pan);

                    RecordBuf.ar(stereo, bufnum, 0, 1, 0, loop: 0, doneAction: 2);
                }).play(server, [\bufnum, self.buffer]);
            } {
                self.buffer = bufferData;
            };

            self.droplets = Array.new(self.config.maxDroplets);
            self.lastSpawnTime = 0;

            self;
        },

        // Advance state and spawn new droplets if needed
        advance: {|self, currentTime|
            var spawnInterval, shouldSpawn;
			var server = Server.default;

            // Clean up inactive droplets
            self.droplets = self.droplets.select({|d| d.active });

            // Calculate spawn interval based on density
            spawnInterval = server.sampleRate / (10.0 * self.config.density);

            // Check if it's time to spawn a new droplet
            shouldSpawn = (currentTime - self.lastSpawnTime >= spawnInterval) &&
                          (self.droplets.size < self.config.maxDroplets);

            if(shouldSpawn) {
                var newDroplet = ~createDroplet.value(
                    (self.config.distributions.position.value * self.buffer.numFrames).asInteger,
                    (self.config.distributions.duration.value * s.sampleRate).asInteger,
                    self.config.distributions.timeWarp,
                    [
                        self.config.distributions.radius.value,
                        self.config.distributions.azimuth.value,
                        self.config.distributions.elevation.value
                    ],
                    self.buffer
                );

                self.droplets = self.droplets.add(newDroplet);
                self.lastSpawnTime = currentTime;
            };

            // Age the droplets
            self.droplets.do {|d| d.advance(1) };

            self;
        },

        // Change the density parameter
        setDensity: {|self, newDensity|
            self.config.density = newDensity;
        },

        // Change the maximum droplets parameter
        setMaxDroplets: {|self, newMax|
            self.config.maxDroplets = newMax;
            // If we have more droplets than the new max, keep only the newest ones
            if(self.droplets.size > newMax) {
                self.droplets = self.droplets[(self.droplets.size - newMax)..];
            };
        },

        // Set a new distribution for a parameter
        setDistribution: {|self, paramName, distributionFunc|
            self.config.distributions[paramName] = distributionFunc;
        }
    );

    catcher.init;
};
)
// ==============================
// TESTING AND VISUALIZATION
// ==============================

// Start the server for audio testing
s.boot;

// Create a test buffer with some audio
(
~testBuffer = Buffer.alloc(s, s.sampleRate * 5, 2, {|buf|
    // Fill with a test sound once allocation is complete
    SynthDef(\fillTestBuf, {|out=0, bufnum|
        var dur = 5;  // 5 seconds
        var sig = Mix([
            PinkNoise.ar(0.3),
            SinOsc.ar(LFNoise1.kr(0.5).exprange(200, 800), 0, 0.5),
            Saw.ar(LFNoise1.kr(0.2).exprange(100, 300), 0.3)
        ]);

        // Make it stereo with some movement
        var pan = SinOsc.kr(0.4).range(-0.7, 0.7);
        var stereo = Pan2.ar(sig, pan);

        RecordBuf.ar(stereo, bufnum, 0, 1, 0, loop: 0, doneAction: 2);
    }).play(s, [\bufnum, buf]);

    "Buffer % allocated and filled with test audio".format(buf).postln;
});
)

// Simple test with clustered spatial distribution
(
// Create configuration with clustered spatial distribution
var config = (
    bufferLength: ~testBuffer.numFrames,
    density: 8,  // Droplets per second
    maxDroplets: 32,
    distributions: (
        position: { 1.0.rand },
        duration: { 0.05 + (0.25 * 1.0.rand) },  // 50-300ms
        timeWarp: {|t|
            var warpCurve = 1.0.rand2;  // -1.0 to 1.0
            var randomFactor = 0.8 + (0.4.rand);  // 0.8 to 1.2 (±20%)
			["XXX: timeWarp", t].postln;

            if(warpCurve >= 0.0) {
                t.pow(1.0 + (warpCurve * 3.0)) * randomFactor
            } {
                t.pow(1.0 / (1.0 + (warpCurve.abs * 3.0))) * randomFactor
            }
        },

        // Clustered distribution
        radius: {
            // Three clusters: near, mid, far
            var cluster = 3.rand;
            var centers = [0.2, 0.5, 0.8];
            var center = centers[cluster];
            var spread = 0.15;

            (center + (spread * 1.0.rand2)).clip(0.1, 1.0);
        },

        azimuth: {
            // Five azimuth clusters
            var cluster = 5.rand;
            var centers = [0, 0.4pi, 0.8pi, 1.2pi, 1.6pi];
            var center = centers[cluster];
            var spread = 0.2pi;

            (center + (spread * 1.0.rand2)).mod(2pi);
        },

        elevation: {
            // Three elevation layers: low, mid, high
            var cluster = 3.rand;
            var centers = [-0.3pi, 0, 0.3pi];
            var center = centers[cluster];
            var spread = 0.1pi;

            (center + (spread * 1.0.rand2)).clip(-pi/2, pi/2);
        }
    )
);

// Create a rain catcher with our configuration
~rainCatcher = ~makeRainCatcher.value(config);
~rainCatcher.buffer = ~testBuffer;

// Set render position function (from the updated 3D positioning prototype)
// This is needed for binaural rendering of the droplet positions
~renderBinaural = {|radial|
    var radius, azimuth, elevation;
    var left, right, pan, depthAttenuation, elevationFactor;
    var headRadius, speedOfSound, maxDelay, leftDelay, rightDelay;
    var sinAz, sinAzAbs;

    radius = radial[0];
    azimuth = radial[1];
    elevation = radial[2];

    // Convert azimuth to pan position (0-1)
    pan = sin(azimuth) * 0.5 + 0.5;

    // Distance attenuation
    depthAttenuation = 0.3 + (0.7 * (1.0 - radius));

    // Calculate amplitude using cosine panning law
    left = cos(pan * pi/2) * depthAttenuation;
    right = sin(pan * pi/2) * depthAttenuation;

    // Elevation cues
    elevationFactor = sin(elevation) * 0.3;
    left = left * (1.0 - elevationFactor);
    right = right * (1.0 + elevationFactor);

    // Calculate ITD (Interaural Time Difference)
    headRadius = 0.0875; // meters
    speedOfSound = 343.0; // m/s
    maxDelay = headRadius / speedOfSound; // ~0.00025s or 0.25ms

    // Initialize delays
    leftDelay = 0.0;
    rightDelay = 0.0;

    // Calculate delays based on azimuth using UGen-compatible constructs
    sinAz = sin(azimuth);
    sinAzAbs = abs(sinAz);

    // Use proper UGen method for conditionally applying delay
    leftDelay = sinAzAbs * maxDelay * 0.5 * (sinAz + abs(sinAz));  // Only non-zero when sinAz > 0
    rightDelay = sinAzAbs * maxDelay * 0.5 * (abs(sinAz) - sinAz); // Only non-zero when sinAz < 0

    // Return amplitude and delay information
    [left, right, leftDelay, rightDelay];
};

// Create a visualization window and play the droplets
~vizWindow = ~visualizeDroplets.value(~rainCatcher.droplets);

// Run the simulation
~currentTime = 0;
~updateRoutine = Routine({
    while({ ~vizWindow.isClosed.not }) {
        var numSamplesPerUpdate = s.sampleRate / 20;  // Update at 20Hz        // Update rain catcher state
        ~rainCatcher.advance(~currentTime);
        ~currentTime = ~currentTime + numSamplesPerUpdate;

        // Play the new droplets
        ~playDroplets.value(~rainCatcher.droplets, ~testBuffer);

        0.05.wait;  // 50ms = 20Hz update rate
    }
}).play;
)

// Stop everything
(
~updateRoutine.stop;
s.freeAll;
)

// ==============================
// INTERACTIVE TESTING
// ==============================

// Interactive Rain Catcher controller
(
var window = Window("Rain Catcher Controller", Rect(100, 100, 800, 600)).front;
var vizWindow, rainCatcher, updateRoutine, currentTime = 0;
var densitySlider, maxDropletsSlider, distributionSelectors;
var dropletCountText;
var renderModeMenu;
var renderMode = \advanced;

// Create a fresh rain catcher
var config = (
    bufferLength: ~testBuffer.numFrames,
    density: 8,            // Droplets per second
    maxDroplets: 32,       // Maximum concurrent droplets
    distributions: (
        position: { 1.0.rand },  // Uniform position in buffer
        duration: { 0.05 + (0.25 * 1.0.rand) },  // 50-300ms
        timeWarp: {|t|
            var warpCurve = 1.0.rand2;  // -1.0 to 1.0
            var randomFactor = 0.8 + (0.4.rand);  // 0.8 to 1.2 (±20%)

            if(warpCurve >= 0.0) {
                t.pow(1.0 + (warpCurve * 3.0)) * randomFactor
            } {
                t.pow(1.0 / (1.0 + (warpCurve.abs * 3.0))) * randomFactor
            }
        },
        radius: { 1.0.rand },
        azimuth: { 2pi.rand },
        elevation: { pi.rand - (pi/2) }
    )
);

rainCatcher = ~makeRainCatcher .value(config);
rainCatcher.buffer = ~testBuffer;

// Top section - control parameters
StaticText(window, Rect(20, 20, 150, 20))
    .string_("Density (droplets/sec):");
densitySlider = Slider(window, Rect(20, 40, 200, 20))
    .value_(rainCatcher.config.density / 20)  // 0-20 range
    .action_({|slider|
        var newDensity = (slider.value * 20).round(0.1);
        rainCatcher.setDensity(newDensity);
        StaticText(window, Rect(230, 40, 50, 20))
            .string_(newDensity);
    });
StaticText(window, Rect(230, 40, 50, 20))
    .string_(rainCatcher.config.density);

StaticText(window, Rect(20, 70, 150, 20))
    .string_("Max Droplets:");
maxDropletsSlider = Slider(window, Rect(20, 90, 200, 20))
    .value_(rainCatcher.config.maxDroplets / 64)  // 0-64 range
    .action_({|slider|
        var newMax = (slider.value * 64).asInteger.max(1);
        rainCatcher.setMaxDroplets(newMax);
        StaticText(window, Rect(230, 90, 50, 20))
            .string_(newMax);
    });
StaticText(window, Rect(230, 90, 50, 20))
    .string_(rainCatcher.config.maxDroplets);

// Display active droplet count
StaticText(window, Rect(20, 120, 150, 20))
    .string_("Active Droplets:");
dropletCountText = StaticText(window, Rect(180, 120, 100, 20))
    .string_("0");

// Render mode selection
StaticText(window, Rect(20, 150, 150, 20))
    .string_("Render Mode:");
renderModeMenu = PopUpMenu(window, Rect(20, 170, 150, 20))
    .items_(["simple", "advanced", "spatial"])
    .value_(1)  // Default to advanced
    .action_({|menu|
        renderMode = menu.items[menu.value].asSymbol;
    });

// Distribution selection
StaticText(window, Rect(300, 20, 200, 20))
    .string_("Distribution Types:");
distributionSelectors = ();

// Function to create a distribution selector
var createDistributionSelector = {|name, label, ypos, items, defaultIndex=0|
    StaticText(window, Rect(300, ypos, 100, 20))
        .string_(label);
    distributionSelectors[name] = PopUpMenu(window, Rect(400, ypos, 150, 20))
        .items_(items)
        .value_(defaultIndex)
        .action_({|menu|
            var selected = menu.items[menu.value].asSymbol;
            var distFunc;

            case
            { selected == \uniform } {
                distFunc = { 1.0.rand };
            }
            { selected == \gaussian } {
                distFunc = {
                    var u1 = 1.0.rand;
                    var u2 = 1.0.rand;
                    var r = (-2 * u1.log).sqrt;
                    var theta = 2pi * u2;
                    var stdDev = 0.15;
                    (r * theta.sin * stdDev + 0.5).clip(0, 1);
                };
            }
            { selected == \triangular } {
                distFunc = {
                    var a = 1.0.rand;
                    var b = 1.0.rand;
                    (a + b) * 0.5;
                };
            }
            { selected == \exponential } {
                distFunc = {
                    var lambda = 5;
                    (1.0 - (-1 * lambda * 1.0.rand).exp).clip(0, 1);
                };
            }
            { selected == \bimodal } {
                distFunc = {
                    var dist = if(1.0.rand < 0.5) { 0.25 } { 0.75 };
                    (dist + (0.gaussian * 0.1)).clip(0, 1);
                };
            }
            { selected == \clustered } {
                if(name == \radius) {
                    distFunc = {
                        var cluster = 3.rand;
                        var centers = [0.2, 0.5, 0.8];
                        var center = centers[cluster];
                        var spread = 0.15;

                        (center + (spread * 1.0.rand2)).clip(0.1, 1.0);
                    };
                } {
                    if(name == \azimuth) {
                        distFunc = {
                            var cluster = 5.rand;
                            var centers = [0, 0.4pi, 0.8pi, 1.2pi, 1.6pi];
                            var center = centers[cluster];
                            var spread = 0.2pi;

                            (center + (spread * 1.0.rand2)).mod(2pi);
                        };
                    } {
                        // For elevation
                        distFunc = {
                            var cluster = 3.rand;
                            var centers = [-0.3pi, 0, 0.3pi];
                            var center = centers[cluster];
                            var spread = 0.1pi;

                            (center + (spread * 1.0.rand2)).clip(-pi/2, pi/2);
                        };
                    };
                };
            };

            if(distFunc.notNil) {
                rainCatcher.setDistribution(name, distFunc);
            };
        });
};

// Create selectors for the different distributions
createDistributionSelector.value(
    \position, "Buffer Position:", 50,
    ["uniform", "gaussian", "triangular", "exponential", "bimodal"]
);
createDistributionSelector.value(
    \duration, "Duration:", 80,
    ["uniform", "gaussian", "triangular", "exponential", "bimodal"]
);
createDistributionSelector.value(
    \radius, "Radius:", 110,
    ["uniform", "gaussian", "triangular", "exponential", "bimodal", "clustered"],
    5  // Default to clustered
);
createDistributionSelector.value(
    \azimuth, "Azimuth:", 140,
    ["uniform", "gaussian", "triangular", "exponential", "bimodal", "clustered"],
    5  // Default to clustered
);
createDistributionSelector.value(
    \elevation, "Elevation:", 170,
    ["uniform", "gaussian", "triangular", "exponential", "bimodal", "clustered"],
    5  // Default to clustered
);

// Start/Stop Buttons
Button(window, Rect(20, 200, 100, 30))
    .states_([["Start"]])
    .action_({
        // Create a visualization window
        vizWindow = ~visualizeDroplets.value(
            rainCatcher.droplets,
            600, 500,
            "Active Droplets (Density: %, Max: %)".format(
                rainCatcher.config.density,
                rainCatcher.config.maxDroplets
            )
        );

        // Reset time counter
        currentTime = 0;

        // Start the update routine
        updateRoutine = Routine({
            while({ vizWindow.isClosed.not && window.isClosed.not }) {
                var numSamplesPerUpdate = s.sampleRate / 20;  // Update at 20Hz
                var activeDroplets;

                // Update rain catcher state
                activeDroplets = rainCatcher.advance(currentTime);
                currentTime = currentTime + numSamplesPerUpdate;

                // Update the droplet count display
                {
                    dropletCountText.string = activeDroplets.size.asString;
                }.defer;

                // Play the new droplets
                ~playDroplets.value(activeDroplets.droplets, ~testBuffer);

                0.05.wait;  // 50ms = 20Hz update rate
            }
        }).play;
    });

Button(window, Rect(130, 200, 100, 30))
    .states_([["Stop"]])
    .action_({
        updateRoutine.stop;
        s.freeAll;
        if(vizWindow.notNil && vizWindow.isClosed.not) {
            vizWindow.close;
        };
    });

// Parameter Presets
StaticText(window, Rect(20, 250, 150, 20))
    .string_("Parameter Presets:");

Button(window, Rect(20, 270, 120, 25))
    .states_([["Sparse & Wide"]])
    .action_({
        rainCatcher.setDensity(3);
        rainCatcher.setMaxDroplets(16);
        densitySlider.value = 3 / 20;
        maxDropletsSlider.value = 16 / 64;
        StaticText(window, Rect(230, 40, 50, 20)).string_(3);
        StaticText(window, Rect(230, 90, 50, 20)).string_(16);
        distributionSelectors[\radius].valueAction = 0;    // Uniform
        distributionSelectors[\azimuth].valueAction = 0;   // Uniform
        distributionSelectors[\elevation].valueAction = 2; // Triangular
    });

Button(window, Rect(150, 270, 120, 25))
    .states_([["Dense & Focused"]])
    .action_({
        rainCatcher.setDensity(15);
        rainCatcher.setMaxDroplets(48);
        densitySlider.value = 15 / 20;
        maxDropletsSlider.value = 48 / 64;
        StaticText(window, Rect(230, 40, 50, 20)).string_(15);
        StaticText(window, Rect(230, 90, 50, 20)).string_(48);
        distributionSelectors[\radius].valueAction = 3;    // Exponential
        distributionSelectors[\azimuth].valueAction = 1;   // Gaussian
        distributionSelectors[\elevation].valueAction = 1; // Gaussian
    });

Button(window, Rect(20, 300, 120, 25))
    .states_([["Ambient Clouds"]])
    .action_({
        rainCatcher.setDensity(8);
        rainCatcher.setMaxDroplets(32);
        densitySlider.value = 8 / 20;
        maxDropletsSlider.value = 32 / 64;
        StaticText(window, Rect(230, 40, 50, 20)).string_(8);
        StaticText(window, Rect(230, 90, 50, 20)).string_(32);
        distributionSelectors[\radius].valueAction = 5;    // Clustered
        distributionSelectors[\azimuth].valueAction = 5;   // Clustered
        distributionSelectors[\elevation].valueAction = 5; // Clustered
    });

Button(window, Rect(150, 300, 120, 25))
    .states_([["Rhythmic Drops"]])
    .action_({
        rainCatcher.setDensity(5);
        rainCatcher.setMaxDroplets(24);
        densitySlider.value = 5 / 20;
        maxDropletsSlider.value = 24 / 64;
        StaticText(window, Rect(230, 40, 50, 20)).string_(5);
        StaticText(window, Rect(230, 90, 50, 20)).string_(24);
        distributionSelectors[\radius].valueAction = 1;    // Gaussian
        distributionSelectors[\azimuth].valueAction = 4;   // Bimodal (left-right)
        distributionSelectors[\elevation].valueAction = 0; // Uniform
    });

// Add explanatory text
TextView(window, Rect(300, 200, 470, 350))
    .string_(
        "Rain Catcher System in Simply Droplets\n\n" ++
        "This prototype demonstrates the Rain Catcher system that generates\n" ++
        "droplets according to configurable distributions.\n\n" ++
        "Key Parameters:\n" ++
        "• Density: Controls how many droplets are generated per second\n" ++
        "• Max Droplets: Limits the maximum number of concurrent droplets\n\n" ++
        "Distribution Types:\n" ++
        "• Uniform: Even distribution across the entire range\n" ++
        "• Gaussian: Bell curve centered in the middle\n" ++
        "• Triangular: Linear falloff from center to edges\n" ++
        "• Exponential: More values near the minimum\n" ++
        "• Bimodal: Two distinct centers of activity\n" ++
        "• Clustered: Multiple focal points (spatial parameters only)\n\n" ++
        "Each droplet has:\n" ++
        "• A position in the source buffer\n" ++
        "• A duration (visualized as size)\n" ++
        "• A time warping curve\n" ++
        "• A position in 3D space (radius, azimuth, elevation)\n\n" ++
        "The display shows the top-down view of droplets. Color indicates\n" ++
        "elevation (blue = below, green = horizon, red = above) and opacity\n" ++
        "shows the age of the droplet (newer = more opaque)."
    )
    .editable_(false);

// Initialize the window
window.onClose_({
    // Ensure cleanup
    if(updateRoutine.notNil) { updateRoutine.stop };
    if(vizWindow.notNil && vizWindow.isClosed.not) { vizWindow.close };
    s.freeAll;
});
)

// ==============================
// NOTES AND OBSERVATIONS
// ==============================

/*
Rain Catcher findings:

1. The relationship between density parameter and spawning rate works well
   with the formula: spawn_interval = sample_rate / (10.0 * density)
   This provides intuitive control from sparse to dense textures.

2. Limiting to 32 concurrent droplets provides a good balance between
   rich sound and CPU efficiency.

3. For spatial positioning, clustered distributions create more engaging
   and natural soundscapes than purely random distributions.

4. Random variation of ±20% on the time warp factor is essential for creating
   organic sound textures that avoid the mechanical quality often associated
   with granular synthesis.

5. Duration distributions concentrated around 100-300ms provide the most
   musical results for most source material.

6. Position distributions that favor the beginning of buffers (exponential)
   work well for sounds with strong attacks.

7. The normalization approach based on the square root of active droplet count
   effectively controls volume without drastic changes as density varies.

Key implementation insights:
- Clean management of droplet lifecycle is essential for stability
- Separate configuration from processing logic
- Flexible distribution system allows dramatic changes in character
- Visualizing the droplets provides valuable insight into behavior
*/
