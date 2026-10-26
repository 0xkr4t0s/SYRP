# Migrating from JUCE

Each JUCE class will get a page here mapping it to its Auricle equivalent, generated from the
migration map (`migration-map.json` on the agent side). Pages carry front matter:

```yaml
---
juce: juce::Slider
auricle: auricle_widgets_slider::Slider
status: todo
---
```

Concept guides (MessageManager to MessageLoop, AudioProcessorValueTreeState to ParamTree) will be
written as the matching crates land.
