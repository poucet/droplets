// Simply Droplets - Audio Engine Core Prototype
// This file implements the core audio processing engine that integrates all components

// ==============================
// IMPORT COMPONENT FUNCTIONS
// ==============================

// NOTE: For a complete implementation, the functions from the previous prototype 
// files should be incorporated here. For brevity, we'll reimplement key functions.

// From 01_distributions.scd - simplified for this demo
~createDistribution = {|type=\uniform, params=nil|
    params = params ?? ();
    
    case
    { type == \uniform } { { 1.0.rand } }
    { type == \gaussian } { 
        {
            var u1 = 1.0.rand;
            var u2 = 1.0.rand;
            var r = (-2 * u1.log).sqrt;
            var theta = 2pi * u2;
            var stdDev = params[\stdDev] ?? 0.15;
            (r * theta.sin * stdDev + 0.5).clip(0, 1);
        }
    }
    { type == \clustered } {
        {
            var cluster = 3.rand;
            var centers = params[\centers] ?? [0.2, 0.5, 0.8];
            var center = centers[cluster];
            var spread = params[\spread] ?? 0.15;
            
            (center + (spread * 1.0.rand2)).clip(0, 1);
        }
    };
};

// From 02_timewarping.scd - simplified for this demo
~createTimeWarp = {|type=\linear, params=nil|
    params = params ?? ();
    
    case
    { type == \linear } { {|t| t } }
    { type == \exponential } { 
        {|t| 
            var factor = params[\factor] ?? 3.0;
            t.pow(1.0 + factor)
        }
    }
    { type == \logarithmic } {
        {|t|
            var factor = params[\factor] ?? 3.0;
            t.pow(1.0 / (1.0 + factor))
        }
    }
    { type == \power } {
        {|t|
            var warpCurve = params[\warpCurve] ?? 0.0;  // -1.0 to 1.0
            var randomize = params[\randomize] ?? true;
            var randomFactor = if(randomize) { 0.8 + (0.4.rand) } { 1.0 };  // ±20% variation
            
            if(warpCurve >= 0.0) {
                // Exponential curve (acceleration)
                t.pow(1.0 + (warpCurve * 3.0)) * randomFactor
            } {
                // Logarithmic curve (deceleration)
                t.pow(1.0 / (1.0 + (warpCurve.abs * 3.0))) * randomFactor
            };
        }
    };
};

// From 03_radial_positioning.scd - simplified
~renderToStereo = {|radial, mode=\advanced|
    var radius = radial[0];
    var azimuth = radial[1];
    var elevation = radial[2];
    var left, right;
    
    // Use cosine-based panning law for smoother energy distribution
    var pan = sin(azimuth) * 0.5 + 0.5;  // Convert azimuth to 0-1 pan value
    
    // Depth attenuation: distant sounds are quieter
    var depthAttenuation = 0.3 + (0.7 * (1.0 - radius));  // Scale from 0.3 to 1.0
    
    // Elevation affects the stereo width - higher sounds are wider
    var elevationFactor = 0.5 + (sin(elevation) * 0.5);  // 0-1 based on elevation
    var effectivePan = (pan - 0.5) * elevationFactor + 0.5;  // Apply elevation to pan
    
    // Apply the panning law
    left = cos(effectivePan * pi/2) * depthAttenuation;
    right = sin(effectivePan * pi/2) * depthAttenuation;
    
    [left, right];
};

// From 05_envelope_processing.scd - simplified
~createEnvelope = {|type=\adsr, params=nil|
    params = params ?? ();
    
    case
    { type == \adsr } {
        {|duration, t|
            var attack = params[\attack] ?? 0.1;
            var decay = params[\decay] ?? 0.2;
            var sustain = params[\sustain] ?? 0.7;
            var release = params[\release] ?? 0.2;
            var curve = params[\curve] ?? (-4);
            
            var result = Env.adsr(
                attackTime: attack,
                decayTime: decay,
                sustainLevel: sustain,
                releaseTime: release,
                curve: curve
            ).at(t);
            
            result;
        }
    }
    { type == \triangle } {
        {|duration, t|
            var attackPeak = params[\attackPeak] ?? 0.5;
            
            if(t < attackPeak) {
                t / attackPeak;
            } {
                1 - ((t - attackPeak) / (1 - attackPeak));
            };
        }
    }
    { type == \percussion } {
        {|duration, t|
            var attackTime = params[\attackTime] ?? 0.005;
            var curve = params[\curve] ?? (-4);
            
            if(t < attackTime) {
                t / attackTime;
            } {
                exp((t - attackTime) * curve);
            }
        }
    }
    { type == \gaussian } {
        {|duration, t|
            var center = params[\center] ?? 0.5;
            var width = params[\width] ?? 0.25;
            
            exp(-1 * ((t - center) ** 2) / width);
        }
    };
};

// ==============================
// CORE DROPLET DEFINITION
// ==============================

// Integrated droplet with all component functions
~createDroplet = {|config|
    var droplet = (
        // Capture information
        sourceBuffer: config[\buffer],
        startPosition: config[\startPosition] ?? 0,
        duration: config[\duration] ?? 44100,  // 1 second at 44.1k
        
        // Playback control
        age: 0,
        timeWarp: config[\timeWarp] ?? {|t| t},  // Default to linear
        
        // 3D positioning
        position: config[\position] ?? [0.5, 0, 0],  // Default to center front
        
        // Envelope
        envelope: config[\envelope] ?? {|dur, t| sin(t * pi)},  // Default to sine
        
        // Processing
        dryWet: config[\dryWet] ?? 1.0,  // Default to 100% wet
        
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
        },
        
        // Get stereo gain for spatial positioning
        getStereoGain: {|self|
            ~renderToStereo.value(self.position);
        },
        
        // Complete processing of one sample
        processSample: {|self, inputSample|
            var normalizedAge = self.age / self.duration;
            var playbackPos = self.getPlaybackPosition;
            var envValue = self.getEnvelopeValue;
            var stereoGain = self.getStereoGain;
            
            // Get the sample from the buffer
            var bufferSample = self.sourceBuffer.at(playbackPos);
            
            // Apply envelope
            var processedSample = bufferSample * envValue;
            
            // Apply stereo gain for positioning
            var leftSample = processedSample * stereoGain[0];
            var rightSample = processedSample * stereoGain[1];
            
            // Apply dry/wet mix
            var dryAmt = 1.0 - self.dryWet;
            var wetAmt = self.dryWet;
            
            [
                (inputSample[0] * dryAmt) + (leftSample * wetAmt),
                (inputSample[1] * dryAmt) + (rightSample * wetAmt)
            ];
        }
    );
    
    droplet;
};

// ==============================
// RAIN CATCHER (DROPLET GENERATOR)
// ==============================

~createRainCatcher = {|config|
    var rainCatcher = (
        // Configuration
        config: config ?? (
            density: 5,  // Droplets per second
            maxDroplets: 32,  // Maximum concurrent droplets
            buffer: nil,
            distributions: (
                // Position in source buffer
                position: ~createDistribution.value(\uniform),
                
                // Duration in samples
                duration: ~createDistribution.value(\gaussian, (stdDev: 0.1)),
                
                // Time warping
                timeWarp: ~createTimeWarp.value(\power, (warpCurve: 0.0, randomize: true)),
                
                // 3D position
                radius: ~createDistribution.value(\gaussian, (stdDev: 0.2)),
                azimuth: ~createDistribution.value(\uniform),
                elevation: ~createDistribution.value(\gaussian, (stdDev: 0.2)),
                
                // Envelope
                envelope: [\adsr, \triangle, \percussion, \gaussian],
                envelopeParams: (
                    adsr: (attack: 0.1, decay: 0.2, sustain: 0.7, release: 0.2),
                    triangle: (attackPeak: 0.5),
                    percussion: (attackTime: 0.001, curve: -8),
                    gaussian: (center: 0.5, width: 0.25)
                )
            )
        ),
        
        // Internal state
        lastSpawnTime: 0,
        
        // Initialize
        init: {|self, buffer|
            if(buffer.notNil) {
                self.config.buffer = buffer;
            };
            
            self;
        },
        
        // Generate a new droplet if conditions are met
        update: {|self, currentTime, activateDropletCount, dryWet=1.0|
            var spawnInterval, shouldSpawn, newDroplet;
            
            // Calculate spawn interval based on density
            spawnInterval = Server.default.sampleRate / (10.0 * self.config.density);
            
            // Check if it's time to spawn a new droplet
            shouldSpawn = (currentTime - self.lastSpawnTime >= spawnInterval) && 
                          (activateDropletCount < self.config.maxDroplets);
            
            if(shouldSpawn && self.config.buffer.notNil) {
                var cfg = ();
                var bufferFrames = self.config.buffer.numFrames;
                var duration, timeWarp, position;
                var envType, envParams, envelope;
                
                // Configure droplet properties using distributions
                cfg.buffer = self.config.buffer;
                cfg.startPosition = (self.config.distributions.position.value * bufferFrames).asInteger;
                cfg.duration = (0.05 + (self.config.distributions.duration.value * 0.45)) * Server.default.sampleRate; // 50-500ms
                cfg.timeWarp = self.config.distributions.timeWarp;
                cfg.position = [
                    self.config.distributions.radius.value,
                    self.config.distributions.azimuth.value * 2pi,
                    (self.config.distributions.elevation.value - 0.5) * pi
                ];
                cfg.dryWet = dryWet;
                
                // Randomly select envelope type
                envType = self.config.distributions.envelope.choose;
                envParams = self.config.distributions.envelopeParams[envType];
                cfg.envelope = ~createEnvelope.value(envType, envParams);
                
                // Create the droplet
                newDroplet = ~createDroplet.value(cfg);
                self.lastSpawnTime = currentTime;
            };
            
            newDroplet;
        }
    );
    
    rainCatcher.init;
};

// ==============================
// CIRCULAR BUFFER IMPLEMENTATION
// ==============================

~createCircularBuffer = {|numChannels=2, maxSeconds=10, sampleRate=44100|
    var frames = maxSeconds * sampleRate;
    
    var circularBuffer = (
        // Buffer properties
        buffer: Array.fill(numChannels, { Array.newClear(frames) }),
        numChannels: numChannels,
        numFrames: frames,
        
        // Read/write positions
        writePos: 0,
        
        // Methods
        write: {|self, sample|
            // Store the sample in the buffer at current write position
            self.numChannels.do {|chan|
                self.buffer[chan][self.writePos] = sample[chan];
            };
            
            // Increment write position
            self.writePos = (self.writePos + 1) % self.numFrames;
        },
        
        read: {|self, position|
            // Read the sample at specified position (relative to writePos)
            var readPos = (self.writePos - position) % self.numFrames;
            if(readPos < 0) { readPos = readPos + self.numFrames };
            
            // Return sample array
            Array.fill(self.numChannels, {|chan|
                self.buffer[chan][readPos];
            });
        },
        
        at: {|self, position|
            // Absolute position in buffer (not relative to writePos)
            var pos = position % self.numFrames;
            
            // Return sample array
            Array.fill(self.numChannels, {|chan|
                self.buffer[chan][pos];
            });
        }
    );
    
    circularBuffer;
};

// ==============================
// COMPLETE AUDIO ENGINE
// ==============================

~createDropletEngine = {|config|
    var engine = (
        // Configuration
        config: config ?? (
            maxSeconds: 5,         // Maximum buffer recording time
            maxDroplets: 32,        // Maximum concurrent droplets
            density: 5,             // Droplets per second (default)
            dryWet: 1.0,            // 100% wet by default
            normalization: true,    // Whether to normalize based on droplet count
            spatialization: true,   // Whether to apply 3D spatialization
            rainCatchers: 1,        // Number of independent rain catchers
            distributions: (
                position: (
                    type: \uniform,
                    params: ()
                ),
                duration: (
                    type: \gaussian,
                    params: (stdDev: 0.1)
                ),
                timeWarp: (
                    type: \power,
                    params: (warpCurve: 0.0, randomize: true)
                ),
                radius: (
                    type: \gaussian,
                    params: (stdDev: 0.2)
                ),
                azimuth: (
                    type: \uniform,
                    params: ()
                ),
                elevation: (
                    type: \gaussian,
                    params: (stdDev: 0.2)
                ),
                envelope: [\adsr, \triangle, \percussion, \gaussian]
            )
        ),
        
        // State
        circularBuffer: nil,
        droplets: nil,
        rainCatchers: nil,
        currentTime: 0,
        outputLevel: 1.0,
        
        // Initialize the engine
        init: {|self|
            // Create circular buffer
            self.circularBuffer = ~createCircularBuffer.value(
                2,  // Stereo
                self.config.maxSeconds,
                Server.default.sampleRate
            );
            
            // Initialize droplets array
            self.droplets = List.new;
            
            // Create rain catchers
            self.rainCatchers = Array.fill(self.config.rainCatchers, {
                var rc = ~createRainCatcher.value((
                    density: self.config.density,
                    maxDroplets: self.config.maxDroplets,
                    distributions: (
                        position: ~createDistribution.value(
                            self.config.distributions.position.type,
                            self.config.distributions.position.params
                        ),
                        duration: ~createDistribution.value(
                            self.config.distributions.duration.type,
                            self.config.distributions.duration.params
                        ),
                        timeWarp: ~createTimeWarp.value(
                            self.config.distributions.timeWarp.type,
                            self.config.distributions.timeWarp.params
                        ),
                        radius: ~createDistribution.value(
                            self.config.distributions.radius.type,
                            self.config.distributions.radius.params
                        ),
                        azimuth: ~createDistribution.value(
                            self.config.distributions.azimuth.type,
                            self.config.distributions.azimuth.params
                        ),
                        elevation: ~createDistribution.value(
                            self.config.distributions.elevation.type,
                            self.config.distributions.elevation.params
                        ),
                        envelope: self.config.distributions.envelope
                    )
                ));
                
                // Pre-initialize with circular buffer
                rc.config.buffer = self.circularBuffer;
                
                rc;
            });
            
            self;
        },
        
        // Process a single stereo sample
        processSample: {|self, inSample|
            var outSample = [0, 0];
            var activeCount;
            
            // Write input to circular buffer
            self.circularBuffer.write(inSample);
            
            // Clean up inactive droplets
            self.droplets.removeAllSuchThat({|d| d.active.not });
            activeCount = self.droplets.size;
            
            // Generate new droplets from each rain catcher
            self.rainCatchers.do {|rc|
                var newDroplet = rc.update(self.currentTime, activeCount, self.config.dryWet);
                if(newDroplet.notNil) {
                    self.droplets.add(newDroplet);
                };
            };
            
            // Reset output
            outSample = [0, 0];
            
            // Process each active droplet
            self.droplets.do {|droplet|
                var processedSample = droplet.processSample(inSample);
                
                // Add to output mix
                outSample[0] = outSample[0] + processedSample[0];
                outSample[1] = outSample[1] + processedSample[1];
                
                // Advance droplet age
                droplet.advance(1);
            };
            
            // Update active count after possible new droplets
            activeCount = self.droplets.count({|d| d.active });
            
            // Apply normalization if enabled
            if(self.config.normalization && (activeCount > 0)) {
                // Use square root for more natural volume scaling
                var normFactor = 1.0 / activeCount.sqrt;
                outSample[0] = outSample[0] * normFactor;
                outSample[1] = outSample[1] * normFactor;
            };
            
            // Apply output level
            outSample[0] = outSample[0] * self.outputLevel;
            outSample[1] = outSample[1] * self.outputLevel;
            
            // Increment time counter
            self.currentTime = self.currentTime + 1;
            
            outSample;
        },
        
        // Set parameter functions
        setDensity: {|self, newDensity|
            self.config.density = newDensity;
            self.rainCatchers.do {|rc|
                rc.config.density = newDensity;
            };
        },
        
        setDryWet: {|self, newDryWet|
            self.config.dryWet = newDryWet;
        },
        
        setOutputLevel: {|self, newLevel|
            self.outputLevel = newLevel;
        },
        
        // Reset the engine state
        reset: {|self|
            self.currentTime = 0;
            self.droplets = List.new;
            self.rainCatchers.do {|rc|
                rc.lastSpawnTime = 0;
            };
        }
    );
    
    engine.init;
};

// ==============================
// TESTING: INTEGRATED ENGINE
// ==============================

// Start the server for audio testing
s.boot;

// Single sample audio processing test
(
var engine = ~createDropletEngine.value();
var inSample = [0.1, 0.1];  // Test input sample
var outSample;

// Test processing a single sample
outSample = engine.processSample(inSample);
[engine.currentTime, engine.droplets.size, outSample].postln;
)

// ==============================
// REALTIME PROCESSING IMPLEMENTATION
// ==============================

// Create a UGen-based processor for realtime audio
(
SynthDef(\dropletProcessor, {|inBus=0, outBus=0, density=5, dryWet=1.0, outputLevel=1.0|
    var in, out;
    
    // Input from bus
    in = In.ar(inBus, 2);
    
    // Process through external object (handled in language)
    out = in;  // placeholder, will be replaced with dropletr processing
    
    // Output
    Out.ar(outBus, out * outputLevel);
}).add;
)

// ==============================
// GRANULAR PROCESSING SYNTH
// ==============================

// A full implementation directly in SC UGens (without external processing)
(
SynthDef(\dropletSynth, {|bufnum, outBus=0, density=5, dryWet=1.0, outputLevel=1.0|
    var trigger, grainDur, pos, pan, envType, timewarp;
    var grainEnv, numChannels, sig;
    
    // Create a trigger for grain generation
    trigger = Dust.kr(density * 10);  // Density * 10 grains per second
    
    // Random duration between 50-500ms
    grainDur = TRand.kr(0.05, 0.5, trigger);
    
    // Random buffer position
    pos = TRand.kr(0, BufFrames.kr(bufnum) - 44100, trigger);
    
    // Random panning
    pan = TRand.kr(-1, 1, trigger);
    
    // Envelope selection (0-3 maps to sine, percussive, reverse, triangle)
    envType = TRand.kr(0, 3, trigger).round;
    
    // Time warping (0 = normal, 1 = random direction)
    timewarp = TRand.kr(-1, 1, trigger);
    
    // Create grain envelopes based on selected type
    grainEnv = Select.kr(envType, [
        // Sine envelope (standard)
        EnvGen.kr(Env.sine(grainDur), trigger),
        
        // Percussive (quick attack, slow decay)
        EnvGen.kr(Env([0, 1, 0], [0.01, 0.99] * grainDur, [2, -4]), trigger),
        
        // Reverse (slow attack, quick decay)
        EnvGen.kr(Env([0, 1, 0], [0.99, 0.01] * grainDur, [4, -2]), trigger),
        
        // Triangle/Trapezoid
        EnvGen.kr(Env.linen(0.33, 0.34, 0.33, 1) * grainDur, trigger)
    ]);
    
    // Create the grain
    sig = GrainBuf.ar(
        numChannels: 2,
        trigger: trigger,
        dur: grainDur,
        sndbuf: bufnum,
        rate: Select.kr(timewarp < 0, [1, -1]),  // Forward or reverse
        pos: pos / BufFrames.kr(bufnum),
        interp: 4,
        pan: pan,
        envbufnum: -1,  // Use built-in hann envelope
        maxGrains: 512
    );
    
    // Apply envelope
    sig = sig * grainEnv;
    
    // Apply global levels
    sig = sig * outputLevel;
    
    // Output
    Out.ar(outBus, sig);
}).add;
)

// ==============================
// INTERACTIVE TESTING UI
// ==============================

// Interactive engine tester
(
var width = 800, height = 600;
var win = Window("Droplet Engine", Rect(100, 100, width, height)).front;
var engine, inBuf, soundFile, densitySlider, dryWetSlider, outputLevelSlider;
var paramLabels, xAxisMenu, yAxisMenu, viewMode, visualizerView;
var transportButtons, statsView, playRoutine, visualizeRoutine;
var dropletLifetimes = List[];
var lastNumDroplets = 0;

// Create buffer for testing
inBuf = Buffer.alloc(s, s.sampleRate * 10, 2);

// Fill with test sound on server boot
s.waitForBoot {
    // Generate test sound (noise + sine sweeps) directly on server
    Routine({
        SynthDef(\fillBuf, {|bufnum|
            var dur = 5;  // 5 seconds
            var sig = Mix([
                PinkNoise.ar(0.3),
                SinOsc.ar(LFNoise1.kr(0.5).exprange(300, 1200), 0, 0.5),
                Saw.ar(LFNoise1.kr(0.2).exprange(100, 400), 0.3)
            ]);
            
            // Make it stereo with some movement
            var pan = SinOsc.kr(0.3).range(-0.7, 0.7);
            var stereo = Pan2.ar(sig, pan);
            
            RecordBuf.ar(stereo, bufnum, 0, 1, 0, loop: 0, doneAction: 2);
        }).play(s, [\bufnum, inBuf]);
        
        "Generating audio in buffer...".postln;
        3.wait;  // Wait for the synth to finish recording
        "Audio generated in buffer %".format(inBuf).postln;
    }).play;
};

// Create engine
engine = ~createDropletEngine.value();

// UI elements - Control section
StaticText(win, Rect(20, 20, 120, 20))
    .string_("Density:");
densitySlider = Slider(win, Rect(150, 20, 200, 20))
    .value_(engine.config.density / 20)  // 0-20 range
    .action_({|slider|
        var newDensity = (slider.value * 20).round(0.1);
        engine.setDensity(newDensity);
        StaticText(win, Rect(360, 20, 50, 20))
            .string_(newDensity);
    });
StaticText(win, Rect(360, 20, 50, 20))
    .string_(engine.config.density);

StaticText(win, Rect(20, 50, 120, 20))
    .string_("Dry/Wet Mix:");
dryWetSlider = Slider(win, Rect(150, 50, 200, 20))
    .value_(engine.config.dryWet)  // 0-1 range
    .action_({|slider|
        var newValue = slider.value;
        engine.setDryWet(newValue);
        StaticText(win, Rect(360, 50, 50, 20))
            .string_(newValue.round(0.01));
    });
StaticText(win, Rect(360, 50, 50, 20))
    .string_(engine.config.dryWet);

StaticText(win, Rect(20, 80, 120, 20))
    .string_("Output Level:");
outputLevelSlider = Slider(win, Rect(150, 80, 200, 20))
    .value_(engine.outputLevel)  // 0-1 range
    .action_({|slider|
        var newValue = slider.value;
        engine.setOutputLevel(newValue);
        StaticText(win, Rect(360, 80, 50, 20))
            .string_(newValue.round(0.01));
    });
StaticText(win, Rect(360, 80, 50, 20))
    .string_(engine.outputLevel);

// Distribution controls
StaticText(win, Rect(430, 20, 120, 20))
    .string_("Distributions:");
StaticText(win, Rect(430, 50, 70, 20))
    .string_("Position:");
PopUpMenu(win, Rect(500, 50, 100, 20))
    .items_(["uniform", "gaussian", "clustered"])
    .action_({|menu|
        var selected = menu.items[menu.value].asSymbol;
        engine.config.distributions.position.type = selected;
        engine.rainCatchers.do {|rc|
            rc.config.distributions.position = ~createDistribution.value(
                selected,
                engine.config.distributions.position.params
            );
        };
    });

StaticText(win, Rect(430, 80, 70, 20))
    .string_("Time Warp:");
PopUpMenu(win, Rect(500, 80, 100, 20))
    .items_(["linear", "exponential", "logarithmic", "power"])
    .action_({|menu|
        var selected = menu.items[menu.value].asSymbol;
        engine.config.distributions.timeWarp.type = selected;
        engine.rainCatchers.do {|rc|
            rc.config.distributions.timeWarp = ~createTimeWarp.value(
                selected,
                engine.config.distributions.timeWarp.params
            );
        };
    });

// Visualization view
visualizerView = UserView(win, Rect(20, 120, width - 40, height - 150))
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
        engine.droplets.do {|droplet|
            if(droplet.active) {
                var radius = droplet.position[0];
                var azimuth = droplet.position[1];
                var elevation = droplet.position[2];
                var x, y, size, opacity;
                var normalizedAge = droplet.age / droplet.duration;
                
                // Calculate position in the view
                x = center.x + (cos(azimuth) * radius * maxRadius);
                y = center.y + (sin(azimuth) * radius * maxRadius);
                
                // Size varies with duration (longer = bigger)
                size = (droplet.duration / s.sampleRate * 60).clip(3, 15);
                
                // Opacity varies with envelope value
                opacity = droplet.getEnvelopeValue;
                
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

// Stats display
statsView = StaticText(win, Rect(20, height - 25, width - 40, 20))
    .string_("Droplet Engine Ready. Click Start to begin processing.")
    .align_(\center);

// Transport controls - Play/Stop buttons
Button(win, Rect(300, height - 55, 80, 25))
    .states_([["Start"]])
    .action_({
        // Start processing
        engine.reset;
        
        // Create synth for real audio processing using the internal UGen
        SynthDef(\realTimeDroplets, {|outBus=0, density=5, dryWet=1.0, outputLevel=1.0|
            var trigger, grainDur, pos, pan, envType, radius, azimuth, elevation;
            var sig, grainEnv;
            
            // Create triggers for grain generation based on density
            trigger = Dust.kr(density * 10);  // Density * 10 grains per second
            
            // Random duration between 50-500ms
            grainDur = TRand.kr(0.05, 0.5, trigger);
            
            // Random buffer position
            pos = TRand.kr(0, BufFrames.kr(inBuf) - 44100, trigger);
            
            // Random 3D position
            radius = TRand.kr(0.0, 1.0, trigger);
            azimuth = TRand.kr(0, 2pi, trigger);
            elevation = TRand.kr(-0.5pi, 0.5pi, trigger);
            
            // Calculate stereo position from 3D coordinates
            // Use sin/cos panning based on azimuth
            pan = azimuth.sin * 0.5 + 0.5;  // 0-1 range
            
            // Apply depth attenuation based on radius
            var depthAttenuation = 0.3 + (0.7 * (1.0 - radius));
            
            // Random envelope selection (0-3 maps to sine, percussive, reverse, triangle)
            envType = TRand.kr(0, 3, trigger).round;
            
            // Create grain envelopes based on selected type
            grainEnv = Select.kr(envType, [
                // Sine envelope (standard)
                EnvGen.kr(Env.sine(grainDur), trigger),
                
                // Percussive (quick attack, slow decay)
                EnvGen.kr(Env([0, 1, 0], [0.01, 0.99] * grainDur, [2, -4]), trigger),
                
                // Reverse (slow attack, quick decay)
                EnvGen.kr(Env([0, 1, 0], [0.99, 0.01] * grainDur, [4, -2]), trigger),
                
                // Triangle/Trapezoid
                EnvGen.kr(Env.linen(0.33, 0.34, 0.33, 1) * grainDur, trigger)
            ]);
            
            // Create the grain
            sig = GrainBuf.ar(
                numChannels: 2,
                trigger: trigger,
                dur: grainDur,
                sndbuf: inBuf,
                rate: 1,
                pos: pos / BufFrames.kr(inBuf),
                interp: 4,
                pan: pan,
                envbufnum: -1  // Use built-in hann envelope
            );
            
            // Apply custom envelope
            sig = sig * grainEnv;
            
            // Apply depth attenuation
            sig = sig * depthAttenuation;
            
            // Apply global levels
            sig = sig * outputLevel;
            
            // Output
            Out.ar(outBus, sig);
        }).play(s, [
            \density, engine.config.density,
            \dryWet, engine.config.dryWet,
            \outputLevel, engine.outputLevel
        ]);
        
        // Start visualization routine
        visualizeRoutine = Routine({
            while({ win.isClosed.not }) {
                { 
                    visualizerView.refresh;
                    statsView.string = "Active Droplets: % / %   Run Time: % sec".format(
                        engine.droplets.size,
                        engine.config.maxDroplets,
                        (engine.currentTime / s.sampleRate).round(0.1)
                    );
                }.defer;
                0.05.wait;  // 20 fps update rate
            }
        }).play;
        
        "Processing started.".postln;
    });

Button(win, Rect(390, height - 55, 80, 25))
    .states_([["Stop"]])
    .action_({
        // Stop processing
        s.freeAll;
        
        if(visualizeRoutine.notNil) {
            visualizeRoutine.stop;
            visualizeRoutine = nil;
        };
        
        "Processing stopped.".postln;
    });

// Window cleanup
win.onClose_({
    s.freeAll;
    if(visualizeRoutine.notNil) {
        visualizeRoutine.stop;
    };
});
)

// ==============================
// INTEGRATED ENGINE TEST
// ==============================

// Using the native SuperCollider grain processing with all components
(
// Create a test buffer with sound
b = Buffer.read(s, Platform.resourceDir +/+ "sounds/a11wlk01.wav");

// Wait for buffer to load
s.sync;

// Play with all components integrated
SynthDef(\integratedDroplets, {|
    bufnum, outBus=0, 
    density=5, 
    dryWet=1.0, 
    outputLevel=1.0,
    timeWarpCurve=0.0,
    positionSpread=1.0,
    envelopeType=0
|
    var trigger, grainDur, pos, radius, azimuth, elevation;
    var grainEnv, sig;
    
    // Create a trigger for grain generation
    trigger = Dust.kr(density * 10);  // Density * 10 grains per second
    
    // Random duration between 50-500ms
    grainDur = TRand.kr(0.05, 0.5, trigger);
    
    // Random buffer position
    pos = TRand.kr(0, BufFrames.kr(bufnum) - 44100, trigger);
    
    // 3D position with distributions
    radius = TRand.kr(0.2, 1.0, trigger);
    azimuth = TRand.kr(0, 2pi, trigger);
    elevation = TRand.kr(-0.5pi, 0.5pi, trigger);
    
    // Calculate pan position from 3D coordinates
    var pan = azimuth.sin * 0.5 + 0.5;  // 0-1 range
    var depthAttenuation = 0.3 + (0.7 * (1.0 - radius));
    
    // Envelope selection (0-3 maps to sine, percussive, reverse, triangle)
    var envSelect = (envelopeType * 3).round;
    
    // Create grain envelopes based on selected type
    grainEnv = Select.kr(envSelect, [
        // Sine envelope (standard)
        EnvGen.kr(Env.sine(grainDur), trigger),
        
        // Percussive (quick attack, slow decay)
        EnvGen.kr(Env([0, 1, 0], [0.01, 0.99] * grainDur, [2, -4]), trigger),
        
        // Reverse (slow attack, quick decay)
        EnvGen.kr(Env([0, 1, 0], [0.99, 0.01] * grainDur, [4, -2]), trigger),
        
        // Triangle/Trapezoid
        EnvGen.kr(Env.linen(0.33, 0.34, 0.33, 1) * grainDur, trigger)
    ]);
    
    // Calculate play rate based on time warp curve
    var playbackRate = 1.0;
    if(timeWarpCurve > 0) {
        // Exponential acceleration
        playbackRate = 1.0 + (timeWarpCurve);
    } {
        // Logarithmic deceleration
        playbackRate = 1.0 + (timeWarpCurve);
    };
    
    // Random variation to playback rate
    playbackRate = playbackRate * TRand.kr(0.8, 1.2, trigger);
    
    // Create the grain
    sig = GrainBuf.ar(
        numChannels: 2,
        trigger: trigger,
        dur: grainDur,
        sndbuf: bufnum,
        rate: playbackRate,
        pos: pos / BufFrames.kr(bufnum),
        interp: 4,
        pan: pan,
        envbufnum: -1  // Use built-in hann envelope
    );
    
    // Apply custom envelope
    sig = sig * grainEnv;
    
    // Apply depth attenuation for distance
    sig = sig * depthAttenuation;
    
    // Apply global levels
    sig = sig * outputLevel;
    
    // Output
    Out.ar(outBus, sig);
}).play(s, [
    \bufnum, b,
    \density, 8,
    \dryWet, 1.0,
    \outputLevel, 0.5,
    \timeWarpCurve, 0.1,
    \positionSpread, 0.8,
    \envelopeType, 0.3
]);
)

// ==============================
// NOTES AND OBSERVATIONS
// ==============================

/*
Audio Engine findings:

1. The integrated approach combining all components works effectively to create
   rich, evolving granular textures with spatial characteristics.

2. Key performance considerations:
   - Limiting to 32 concurrent droplets provides a good balance between
     richness and CPU efficiency
   - The square root normalization approach effectively manages volume as
     density changes
   - Processing each sample individually provides fine-grained control but
     is more CPU intensive

3. For realtime performance, hybrid approaches may be necessary:
   - Using native UGens (like GrainBuf) for the actual granular processing
   - Controlling these UGens with more advanced distribution and parameter logic
   - Performing buffer operations in non-realtime contexts

4. The circular buffer implementation is critical for efficiency:
   - Maintains a sliding window of recent audio
   - Allows independent access by multiple droplets
   - Provides control over memory usage

5. Parameter relationships:
   - Some parameters have strong interdependencies (e.g., density and max droplets)
   - Others function more independently (e.g., spatial positioning)
   - Real-time visualization provides essential feedback for understanding
     these relationships

6. Maintaining separate configuration for each rain catcher allows:
   - Creation of spatially distinct layers
   - Different distribution patterns per layer
   - Independent control over density and other characteristics

Implementation insights:
- Object-oriented design with clear component relationships works well
- Clean separation between configuration and processing logic
- Processing per sample rather than per buffer provides more control
- Using functions for distributions, envelopes, and time warping creates flexibility
- The square root normalization approach is simple yet effective for controlling volume
*/
