import QtQuick
import QtTest
import "../../../crates/app/qml" as Lipa
import "../../../crates/app/tests/SettingsFixture.js" as Fixture

TestCase {
    name: "OverlayLifecycleRegression"
    when: windowShown
    Component { id: windowComponent; Lipa.TranslationWindow {} }
    function test_nativeResizeSurvivesSettingsEchoAndModeSwitch() {
        const settings = Fixture.make({"translation_window.mode": "floating", "translation_window.size": [600, 140]})
        const overlay = createTemporaryObject(windowComponent, this, {settings: settings, shown: true, translation: "Hello"})
        verify(overlay)
        const floating = overlay.window
        floating.width = 720
        floating.height = 220
        overlay.settings = Fixture.patch(settings, {"translation_window.floating_geometry":
            {x: 200, y: 100, w: 720, h: 220, output: "", output_x: 0, output_y: 0}})
        overlay.translation = "A new translation with very different text length".repeat(8)
        wait(30)
        compare([floating.width, floating.height], [720, 220])
        overlay.settings = Fixture.patch(overlay.settings, {"translation_window.mode": "pinned"})
        verify(overlay.window !== floating)
        overlay.settings = Fixture.patch(overlay.settings, {"translation_window.mode": "floating"})
        compare(overlay.window, floating)
        compare([floating.width, floating.height], [720, 220])
        overlay.settings = Fixture.patch(overlay.settings, {"translation_window.size": [650, 160]})
        compare([floating.width, floating.height], [650, 160], "explicit size edits still apply")
    }
    function test_savedSizeRespectsRememberGeometry_data() {
        return [{tag: "enabled", enabled: true, expected: [720, 220]},
                {tag: "disabled", enabled: false, expected: [600, 140]}]
    }
    function test_savedSizeRespectsRememberGeometry(data) {
        const overlay = createTemporaryObject(windowComponent, this, {settings: Fixture.make({
            "translation_window.mode": "floating", "translation_window.size": [600, 140],
            "general.remember_window_geometry": data.enabled,
            "translation_window.floating_geometry": {x: 200, y: 100, w: 720, h: 220, output: "", output_x: 0, output_y: 0}
        }), shown: true})
        verify(overlay)
        compare([overlay.window.width, overlay.window.height], data.expected)
    }
}
