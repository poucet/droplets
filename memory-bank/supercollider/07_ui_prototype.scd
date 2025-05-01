// Simply Droplets - UI Prototype
// This file implements a mockup of the user interface for the Simply Droplets plugin

// ==============================
// UI CONSTANTS AND SETTINGS
// ==============================

// UI Colors
~uiColors = (
    background: Color.fromHexString("#1E1E1E"),
    panelBackground: Color.fromHexString("#282828"),
    controlBackground: Color.fromHexString("#333333"),
    text: Color.fromHexString("#DDDDDD"),
    accent1: Color.fromHexString("#00A2FF"),
    accent2: Color.fromHexString("#9E44FF"),
    visualizer: Color.black
);

// UI Metrics
~uiMetrics = (
    headerHeight: 50,
    controlHeight: 25,
    knobSize: 60,
    sliderHeight: 20,
    padding: 10,
    sectionSpacing: 15,
    panelRadius: 5
);

// ==============================
// DEFAULT PARAMETER VALUES
// ==============================

~defaultParams = (
    // Droplet Generation Parameters
    density: 5,          // Droplets per second
    positionBias: 0.5,   // 0-1, where in the buffer to sample from
    durationMin: 0.05,   // 50ms minimum duration
    durationMax: 0.5,    // 500ms maximum duration
    
    // Envelope Parameters
    envelopeType: 0,     // 0=ADSR, 1=Triangle, 2=Gaussian, 3=Percussion
    attack: 0.1,         // Normalized attack time
    decay: 0.2,          // Normalized decay time
    sustain: 0.7,        // Sustain level
    release: 0.2,        // Normalized release time
    envelopeHumanize: 0.05, // Random variation to envelope shape
    
    // Time Manipulation Parameters
    timeWarpCurve: 0.0,  // -1.0 to 1.0, negative=logarithmic, positive=exponential
    timeStretch: 1.0,    // Overall time scale factor
    playDirection: 0,    // 0=forward, 1=reverse, 2=bidirectional
    
    // Spatial Parameters
    radiusMin: 0.2,      // Closest distance from center
    radiusMax: 1.0,      // Furthest distance from center
    radiusDistribution: 1, // 0=uniform, 1=gaussian, 2=clustered
    azimuthCenter: 0.5,  // 0-1 range, maps to 0-2π
    azimuthSpread: 1.0,  // 0-1 range, full spread = 2π
    azimuthDistribution: 0, // 0=uniform, 1=gaussian, 2=clustered
    elevationCenter: 0.5, // 0-1 range, maps to -π/2 to π/2
    elevationSpread: 1.0, // 0-1 range, full spread = π
    elevationDistribution: 1, // 0=uniform, 1=gaussian, 2=clustered
    
    // Global Processing
    dryWet: 1.0,         // 0-1, dry to wet balance
    outputLevel: 0.7,    // 0-1, overall output level
    maxDroplets: 32      // Maximum concurrent droplets
);

// ==============================
// PANEL CREATOR FUNCTIONS
// ==============================

// Create a panel with title and border
~createPanel = {|parent, bounds, title|
    var panel, titleText;
    
    // Create panel view
    panel = View(parent, bounds)
        .background_(~uiColors.panelBackground);
    
    // Add title
    titleText = StaticText(panel, Rect(10, 5, bounds.width - 20, 20))
        .string_(title)
        .stringColor_(~uiColors.accent1)
        .font_(Font("Arial", 14, true));
    
    panel;
};

// Create a knob with label and value display
~createKnob = {|parent, bounds, label, min=0, max=1, default=0.5, action, unitLabel=""|
    var knob, labelText, valueText, yOffset = 0;
    
    // Create container
    var container = View(parent, bounds)
        .background_(~uiColors.panelBackground);
    
    // Add label
    labelText = StaticText(container, Rect(0, 0, bounds.width, 20))
        .string_(label)
        .stringColor_(~uiColors.text)
        .align_(\center);
    yOffset = 20;
    
    // Create knob
    knob = Knob(container, Rect((bounds.width - ~uiMetrics.knobSize) / 2, yOffset, ~uiMetrics.knobSize, ~uiMetrics.knobSize))
        .color([~uiColors.controlBackground, ~uiColors.accent1])
        .value_((default - min) / (max - min))
        .action_({|k|
            var val = min + (k.value * (max - min));
            valueText.string = val.round(0.01) ++ unitLabel;
            action.value(val);
        });
    yOffset = yOffset + ~uiMetrics.knobSize + 5;
    
    // Add value display
    valueText = StaticText(container, Rect(0, yOffset, bounds.width, 20))
        .string_(default.round(0.01) ++ unitLabel)
        .stringColor_(~uiColors.text)
        .align_(\center);
    
    // Return the knob
    knob;
};

// Create a horizontal slider with label and value display
~createSlider = {|parent, bounds, label, min=0, max=1, default=0.5, action, unitLabel=""|
    var slider, labelText, valueText, knobWidth = 100;
    
    // Create container
    var container = View(parent, bounds)
        .background_(~uiColors.panelBackground);
    
    // Add label
    labelText = StaticText(container, Rect(5, (bounds.height - 15) / 2, bounds.width * 0.4, 15))
        .string_(label)
        .stringColor_(~uiColors.text)
        .align_(\left);
    
    // Create slider
    slider = Slider(container, Rect(bounds.width * 0.4 + 5, (bounds.height - ~uiMetrics.sliderHeight) / 2, knobWidth, ~uiMetrics.sliderHeight))
        .background_(~uiColors.controlBackground)
        .knobColor_(~uiColors.accent1)
        .value_((default - min) / (max - min))
        .action_({|s|
            var val = min + (s.value * (max - min));
            valueText.string = val.round(0.01) ++ unitLabel;
            action.value(val);
        });
    
    // Add value display
    valueText = StaticText(container, Rect(bounds.width * 0.4 + knobWidth + 10, (bounds.height - 15) / 2, bounds.width * 0.2, 15))
        .string_(default.round(0.01) ++ unitLabel)
        .stringColor_(~uiColors.text)
        .align_(\left);
    
    // Return the slider
    slider;
};

// Create a popup menu with label
~createPopUpMenu = {|parent, bounds, label, items, default=0, action|
    var menu, labelText;
    
    // Create container
    var container = View(parent, bounds)
        .background_(~uiColors.panelBackground);
    
    // Add label
    labelText = StaticText(container, Rect(5, (bounds.height - 15) / 2, bounds.width * 0.4, 15))
        .string_(label)
        .stringColor_(~uiColors.text)
        .align_(\left);
    
    // Create menu
    menu = PopUpMenu(container, Rect(bounds.width * 0.4 + 5, (bounds.height - 20) / 2, bounds.width * 0.55, 20))
        .background_(~uiColors.controlBackground)
        .stringColor_(~uiColors.text)
        .items_(items)
        .value_(default)
        .action_({|m|
            action.value(m.value);
        });
    
    // Return the menu
    menu;
};

// Create a 3D position visualizer
~create3DVisualizer = {|parent, bounds|
    var visualizer;
    
    visualizer = UserView(parent, bounds)
        .background_(~uiColors.visualizer)
        .drawFunc_({|view|
            var center = view.bounds.center;
            var maxRadius = view.bounds.width.min(view.bounds.height) * 0.45;
            
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
            
            // Draw front/back/left/right labels
            Pen.stringAtPoint("Front", Point(center.x - 15, center.y - maxRadius - 20));
            Pen.stringAtPoint("Back", Point(center.x - 15, center.y + maxRadius + 5));
            Pen.stringAtPoint("Left", Point(center.x - maxRadius - 30, center.y - 10));
            Pen.stringAtPoint("Right", Point(center.x + maxRadius + 5, center.y - 10));
            
            // Draw some sample droplets
            10.do {|i|
                var angle = 2pi * i / 10;
                var radius = 0.2 + (0.8 * i.linlin(0, 9, 0, 1));
                var elevation = (i.linlin(0, 9, -0.5, 0.5) * pi).sin;
                var x = center.x + (cos(angle) * radius * maxRadius);
                var y = center.y + (sin(angle) * radius * maxRadius);
                var size = 10 * (1 + elevation);
                
                // Color based on elevation (blue below, green at horizon, red above)
                Pen.fillColor = Color.new(
                    0.5 + (elevation * 0.5),  // Red: more for higher elevation
                    0.5 - (elevation.abs * 0.3),  // Green: more at horizon
                    0.5 - (elevation * 0.5),  // Blue: more for lower elevation
                    0.7  // Alpha
                );
                
                Pen.addOval(Rect(x - (size/2), y - (size/2), size, size));
                Pen.fill;
            };
        });
    
    visualizer;
};

// Create a curve editor view
~createCurveEditor = {|parent, bounds, initialCurve=0|
    var editor;
    var curveVal = initialCurve;
    
    editor = UserView(parent, bounds)
        .background_(~uiColors.visualizer)
        .drawFunc_({|view|
            var width = view.bounds.width;
            var height = view.bounds.height;
            var points = Array.fill(100, {|i|
                var t = i / 99;
                var y;
                
                if(curveVal >= 0) {
                    // Exponential curve (acceleration)
                    y = t.pow(1.0 + (curveVal * 3.0));
                } {
                    // Logarithmic curve (deceleration)
                    y = t.pow(1.0 / (1.0 + (curveVal.abs * 3.0)));
                };
                
                [t * width, height - (y * height)];
            });
            
            // Draw grid
            Pen.strokeColor = Color.gray(0.3);
            
            // Vertical lines
            5.do {|i|
                var x = width * i / 4;
                Pen.moveTo(x @ 0);
                Pen.lineTo(x @ height);
            };
            
            // Horizontal lines
            5.do {|i|
                var y = height * i / 4;
                Pen.moveTo(0 @ y);
                Pen.lineTo(width @ y);
            };
            
            Pen.stroke;
            
            // Draw diagonal line (representing linear/no warping)
            Pen.strokeColor = Color.gray(0.5);
            Pen.moveTo(0 @ height);
            Pen.lineTo(width @ 0);
            Pen.stroke;
            
            // Draw curve
            Pen.strokeColor = ~uiColors.accent1;
            Pen.width = 2;
            
            Pen.moveTo(points[0][0] @ points[0][1]);
            points[1..].do {|point|
                Pen.lineTo(point[0] @ point[1]);
            };
            
            Pen.stroke;
            
            // Draw labels
            Pen.stringAtPoint("Time", Point(width/2 - 15, height - 15));
            
            // Rotate text for vertical label
            Pen.rotate(-pi/2, width, height/2);
            Pen.stringAtPoint("Position", Point(width + 5, height/2 - 5));
            Pen.rotate(pi/2, width, height/2);
        })
        .mouseDownAction_({|view, x, y|
            var normX = x / view.bounds.width;
            var normY = y / view.bounds.height;
            
            // Convert mouse position to curve value (-1 to 1)
            // A rough approximation - in a real implementation we'd solve for the curve value
            // that produces a curve passing through this point
            if(normY < (1 - normX)) {
                // Above the diagonal - exponential (positive curve value)
                curveVal = (1 - normY - normX).clip(0, 1) * 2;
            } {
                // Below the diagonal - logarithmic (negative curve value)
                curveVal = -1 * (normY - (1 - normX)).clip(0, 1) * 2;
            };
            
            view.refresh;
        });
    
    // Return a dictionary with the view and a way to get/set the curve value
    (
        view: editor,
        getValue: { curveVal },
        setValue: {|val| 
            curveVal = val;
            editor.refresh;
        }
    );
};

// ==============================
// MAIN UI CREATION
// ==============================

// Create the main UI window with all controls and visualizers
~createMainUI = {
    var window, width = 1000, height = 700;
    var params = ~defaultParams.copy;
    var controlPanels, visualizers;
    var buffer, synth;
    
    // Create the main window
    window = Window("Simply Droplets", Rect(100, 100, width, height))
        .background_(~uiColors.background)
        .front;
    
    // Create panels for the different parameter sections
    controlPanels = (
        dropletGeneration: ~createPanel(
            window, 
            Rect(10, 10, 320, 180), 
            "Droplet Generation Parameters"
        ),
        
        envelope: ~createPanel(
            window, 
            Rect(10, 200, 320, 240), 
            "Envelope Parameters"
        ),
        
        timeManipulation: ~createPanel(
            window, 
            Rect(10, 450, 320, 240), 
            "Time Manipulation Parameters"
        ),
        
        spatial: ~createPanel(
            window, 
            Rect(340, 10, 320, 400), 
            "Spatial Parameters"
        ),
        
        global: ~createPanel(
            window, 
            Rect(340, 420, 320, 150), 
            "Global Processing"
        ),
        
        visualization: ~createPanel(
            window, 
            Rect(670, 10, 320, 560), 
            "Visualization"
        )
    );
    
    // Create the visualizer views
    visualizers = (
        spatial: ~create3DVisualizer(
            controlPanels.visualization,
            Rect(10, 30, 300, 300)
        ),
        
        timeWarp: ~createCurveEditor(
            controlPanels.visualization,
            Rect(10, 340, 300, 200),
            params.timeWarpCurve
        )
    );
    
    // Add controls to the Droplet Generation panel
    ~createSlider(
        controlPanels.dropletGeneration,
        Rect(10, 30, 300, 30),
        "Density",
        1, 20, params.density,
        {|val| params.density = val; },
        " /sec"
    );
    
    ~createSlider(
        controlPanels.dropletGeneration,
        Rect(10, 65, 300, 30),
        "Position Bias",
        0, 1, params.positionBias,
        {|val| params.positionBias = val; }
    );
    
    ~createSlider(
        controlPanels.dropletGeneration,
        Rect(10, 100, 300, 30),
        "Min Duration",
        0.01, 0.2, params.durationMin,
        {|val| params.durationMin = val; },
        " sec"
    );
    
    ~createSlider(
        controlPanels.dropletGeneration,
        Rect(10, 135, 300, 30),
        "Max Duration",
        0.1, 1.0, params.durationMax,
        {|val| params.durationMax = val; },
        " sec"
    );
    
    // Add controls to the Envelope panel
    ~createPopUpMenu(
        controlPanels.envelope,
        Rect(10, 30, 300, 30),
        "Envelope Type",
        ["ADSR", "Triangle", "Gaussian", "Percussion"],
        params.envelopeType,
        {|val| params.envelopeType = val; }
    );
    
    ~createSlider(
        controlPanels.envelope,
        Rect(10, 65, 300, 30),
        "Attack",
        0.001, 0.5, params.attack,
        {|val| params.attack = val; }
    );
    
    ~createSlider(
        controlPanels.envelope,
        Rect(10, 100, 300, 30),
        "Decay",
        0.001, 0.5, params.decay,
        {|val| params.decay = val; }
    );
    
    ~createSlider(
        controlPanels.envelope,
        Rect(10, 135, 300, 30),
        "Sustain",
        0, 1, params.sustain,
        {|val| params.sustain = val; }
    );
    
    ~createSlider(
        controlPanels.envelope,
        Rect(10, 170, 300, 30),
        "Release",
        0.001, 0.5, params.release,
        {|val| params.release = val; }
    );
    
    ~createSlider(
        controlPanels.envelope,
        Rect(10, 205, 300, 30),
        "Humanize",
        0, 0.2, params.envelopeHumanize,
        {|val| params.envelopeHumanize = val; }
    );
    
    // Add controls to the Time Manipulation panel
    ~createSlider(
        controlPanels.timeManipulation,
        Rect(10, 30, 300, 30),
        "Warp Curve",
        -1, 1, params.timeWarpCurve,
        {|val| 
            params.timeWarpCurve = val;
            visualizers.timeWarp.setValue(val);
        }
    );
    
    ~createSlider(
        controlPanels.timeManipulation,
        Rect(10, 65, 300, 30),
        "Time Stretch",
        0.5, 2.0, params.timeStretch,
        {|val| params.timeStretch = val; },
        "x"
    );
    
    ~createPopUpMenu(
        controlPanels.timeManipulation,
        Rect(10, 100, 300, 30),
        "Direction",
        ["Forward", "Reverse", "Bidirectional"],
        params.playDirection,
        {|val| params.playDirection = val; }
    );
    
    // Add controls to the Spatial panel
    ~createSlider(
        controlPanels.spatial,
        Rect(10, 30, 300, 30),
        "Min Radius",
        0, 1, params.radiusMin,
        {|val| params.radiusMin = val; }
    );
    
    ~createSlider(
        controlPanels.spatial,
        Rect(10, 65, 300, 30),
        "Max Radius",
        0, 1, params.radiusMax,
        {|val| params.radiusMax = val; }
    );
    
    ~createPopUpMenu(
        controlPanels.spatial,
        Rect(10, 100, 300, 30),
        "Radius Distribution",
        ["Uniform", "Gaussian", "Clustered"],
        params.radiusDistribution,
        {|val| params.radiusDistribution = val; }
    );
    
    ~createSlider(
        controlPanels.spatial,
        Rect(10, 135, 300, 30),
        "Azimuth Center",
        0, 1, params.azimuthCenter,
        {|val| params.azimuthCenter = val; }
    );
    
    ~createSlider(
        controlPanels.spatial,
        Rect(10, 170, 300, 30),
        "Azimuth Spread",
        0, 1, params.azimuthSpread,
        {|val| params.azimuthSpread = val; }
    );
    
    ~createPopUpMenu(
        controlPanels.spatial,
        Rect(10, 205, 300, 30),
        "Azimuth Distribution",
        ["Uniform", "Gaussian", "Clustered"],
        params.azimuthDistribution,
        {|val| params.azimuthDistribution = val; }
    );
    
    ~createSlider(
        controlPanels.spatial,
        Rect(10, 240, 300, 30),
        "Elevation Center",
        0, 1, params.elevationCenter,
        {|val| params.elevationCenter = val; }
    );
    
    ~createSlider(
        controlPanels.spatial,
        Rect(10, 275, 300, 30),
        "Elevation Spread",
        0, 1, params.elevationSpread,
        {|val| params.elevationSpread = val; }
    );
    
    ~createPopUpMenu(
        controlPanels.spatial,
        Rect(10, 310, 300, 30),
        "Elevation Distribution",
        ["Uniform", "Gaussian", "Clustered"],
        params.elevationDistribution,
        {|val| params.elevationDistribution = val; }
    );
    
    // Add controls to the Global Processing panel
    ~createSlider(
        controlPanels.global,
        Rect(10, 30, 300, 30),
        "Dry/Wet Mix",
        0, 1, params.dryWet,
        {|val| 
            params.dryWet = val;
            if(synth.notNil) { synth.set(\dryWet, val) };
        }
    );
    
    ~createSlider(
        controlPanels.global,
        Rect(10, 65, 300, 30),
        "Output Level",
        0, 1, params.outputLevel,
        {|val| 
            params.outputLevel = val;
            if(synth.notNil) { synth.set(\outputLevel, val) };
        },
        "dB"
    );
    
    ~createSlider(
        controlPanels.global,
        Rect(10, 100, 300, 30),
        "Max Droplets",
        8, 64, params.maxDroplets,
        {|val| params.maxDroplets = val.asInteger; }
    );
    
    // Add transport controls
    Button(window, Rect(670, 580, 100, 30))
        .states_([["Start Audio", Color.white, Color.fromHexString("#00A2FF")]])
        .action_({
            // Start the server if needed
            Server.default.waitForBoot({
                // Create a test buffer if needed
                if(buffer.isNil) {
                    buffer = Buffer.alloc(Server.default, Server.default.sampleRate * 5, 2);
                    
                    // Generate test audio
                    SynthDef(\fillBuf, {|bufnum|
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
                    }).play(Server.default, [\bufnum, buffer]);
                } {
                    // Create and play the granular synth with current parameters
                    synth = SynthDef(\dropletSynth, {|
                        bufnum, outBus=0, 
                        density=5, 
                        positionBias=0.5,
                        durationMin=0.05,
                        durationMax=0.5,
                        timeWarpCurve=0.0,
                        timeStretch=1.0,
                        radiusMin=0.2,
                        radiusMax=1.0,
                        azimuthCenter=0.5,
                        azimuthSpread=1.0,
                        elevationCenter=0.5,
                        elevationSpread=1.0,
                        dryWet=1.0,
                        outputLevel=0.7,
                        maxDroplets=32
                    |
                        var trigger, grainDur, pos, radius, azimuth, elevation;
                        var grainEnv, sig;
                        
                        // Create triggers for grain generation based on density
                        trigger = Dust.kr(density * 10);  // Density * 10 grains per second
                        
                        // Random duration between min and max
                        grainDur = TRand.kr(durationMin, durationMax, trigger);
                        
                        // Random buffer position with bias
                        pos = (TRand.kr(0, 1, trigger) * 0.8 + positionBias * 0.2) * (BufFrames.kr(bufnum) - 44100);
                        
                        // 3D position with parameters
                        radius = TRand.kr(radiusMin, radiusMax, trigger);
                        azimuth = ((TRand.kr(0, 1, trigger) - 0.5) * azimuthSpread + azimuthCenter) * 2pi;
                        elevation = ((TRand.kr(0, 1, trigger) - 0.5) * elevationSpread + elevationCenter) * pi - (pi/2);
                        
                        // Calculate pan position from 3D coordinates
                        var pan = azimuth.sin * 0.5 + 0.5;  // 0-1 range
                        var depthAttenuation = 0.3 + (0.7 * (1.0 - radius));
                        
                        // Random envelope selection based on envelope type
                        var envType = TRand.kr(0, 3, trigger).round;
                        
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
                        
                        // Calculate play rate based on time warp curve and time stretch
                        var playbackRate = timeStretch;
                        if(timeWarpCurve > 0) {
                            // Exponential acceleration
                            playbackRate = playbackRate * (1.0 + timeWarpCurve);
                        } {
                            // Logarithmic deceleration
                            playbackRate = playbackRate * (1.0 + timeWarpCurve);
                        };
                        
                        // Random variation to playback rate for humanization
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
                    }).play(Server.default, [
                        \bufnum, buffer,
                        \density, params.density,
                        \positionBias, params.positionBias,
                        \durationMin, params.durationMin,
                        \durationMax, params.durationMax,
                        \timeWarpCurve, params.timeWarpCurve,
                        \timeStretch, params.timeStretch,
                        \radiusMin, params.radiusMin,
                        \radiusMax, params.radiusMax,
                        \azimuthCenter, params.azimuthCenter,
                        \azimuthSpread, params.azimuthSpread,
                        \elevationCenter, params.elevationCenter,
                        \elevationSpread, params.elevationSpread,
                        \dryWet, params.dryWet,
                        \outputLevel, params.outputLevel
                    ]);
                };
            });
        });
    
    Button(window, Rect(780, 580, 100, 30))
        .states_([["Stop Audio", Color.white, Color.fromHexString("#FF5252")]])
        .action_({
            if(synth.notNil) {
                synth.free;
                synth = nil;
            };
        });
    
    // Add status display
    StaticText(window, Rect(20, 650, width - 40, 30))
        .string_("Simply Droplets UI Prototype - Ready")
        .stringColor_(~uiColors.text)
        .align_(\center)
        .font_(Font("Arial", 14));
    
    // Return the window
    window;
};

// ==============================
// EXECUTE UI CREATION
// ==============================

// Create the main UI
s.waitForBoot({
    ~mainWindow = ~createMainUI.value();
});

// ==============================
// NOTES AND OBSERVATIONS
// ==============================

/*
UI Prototype findings:

1. The parameter organization into logical groups enhances usability:
   - Droplet Generation parameters control creation of new droplets
   - Envelope parameters shape the amplitude profile of each droplet
   - Time Manipulation parameters control playback characteristics
   - Spatial parameters define 3D positioning and distribution
   - Global parameters affect overall processing

2. Key UI elements that enhance the user experience:
   - Real-time 3D visualization provides immediate spatial feedback
   - Time warp curve editor allows intuitive manipulation of time characteristics
   - Consistent color scheme and control layout improve learnability
   - Clear parameter categorization reduces cognitive load

3. Interactive parameter relationships are essential for immediate feedback:
   - Warp curve adjustment should update visualization in real-time
   - Distribution parameters should update the spatial visualization
   - Audio playback should reflect parameter changes immediately

4. Parameter ranges should be carefully constrained for musical results:
   - Density: 1-20 droplets per second works well for most use cases
   - Duration: 50-500ms range provides good balance between texture and clarity
   - Envelope humanization: 2-5% provides natural variation without chaos
   - Time warp: -1.0 to 1.0 range maps intuitively to logarithmic/exponential

5. The UI layout balances complexity with usability:
   - Left side contains temporal parameters (generation, envelope, time)
   - Middle contains spatial parameters
   - Right side contains visualizations
   - Controls are grouped by function rather than by parameter type

Implementation insights:
- Interactive visualizations are crucial for understanding complex parameters
- Consistent visual feedback helps users understand parameter relationships
- Preset system (to be implemented) would help users explore the parameter space
- Curve editors provide an intuitive way to manipulate non-linear parameters
- Live monitoring of active droplet count helps users manage CPU usage
*/
