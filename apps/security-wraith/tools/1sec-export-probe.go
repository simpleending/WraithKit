package main

import (
	"context"
	"encoding/json"
	"fmt"
	"github.com/1sec-project/1sec/internal/core"
	"github.com/1sec-project/1sec/internal/modules/auth"
	"github.com/rs/zerolog"
	"os"
	"time"
)

func must(err error) {
	if err != nil {
		panic(err)
	}
}
func save(name string, v any) {
	b, e := json.MarshalIndent(v, "", "  ")
	must(e)
	must(os.WriteFile("/output/"+name, append(b, '\n'), 0600))
}
func main() {
	cfg := core.DefaultConfig()
	cfg.Bus.DataDir = "/output/nats"
	cfg.Bus.Port = 14222
	bus, e := core.NewEventBus(&cfg.Bus, zerolog.Nop())
	must(e)
	defer bus.Close()
	events := make(chan *core.SecurityEvent, 8)
	must(bus.SubscribeToAllEvents(func(e *core.SecurityEvent) { events <- e }))
	pipeline := core.NewAlertPipeline(zerolog.Nop(), 16)
	module := auth.New()
	must(module.Start(context.Background(), bus, pipeline, cfg))
	defer module.Stop()
	input := core.NewSecurityEvent("synthetic_export_test", "auth_downgrade", core.SeverityInfo, "Synthetic authentication downgrade observation")
	input.Source = "offline-synthetic-test"
	input.SourceIP = "192.0.2.44"
	input.Details = map[string]interface{}{"username": "synthetic-release-bot", "from_method": "passkey", "to_method": "password", "reason": "offline export compatibility test"}
	must(module.HandleEvent(input))
	var emitted *core.SecurityEvent
	select {
	case emitted = <-events:
	case <-time.After(5 * time.Second):
		panic("no generated event")
	}
	alerts := pipeline.GetAlerts(core.SeverityInfo, 16)
	if len(alerts) != 1 || alerts[0].Type != "auth_method_downgrade" || alerts[0].EventIDs[0] != emitted.ID {
		panic("unexpected actual upstream detection")
	}
	save("events.json", []*core.SecurityEvent{emitted})
	save("alerts.json", map[string]any{"alerts": alerts, "total": len(alerts)})
	save("input-event.json", input)
	save("probe-receipt.json", map[string]any{"upstream_commit": "2f6527d3f5e760b0e17955d75043dfb370f75d54", "actual_module": "auth_fortress", "synthetic_inputs": true, "network": "container none; embedded bus loopback only", "engine_started": false, "api_server_started": false, "enforcement_handlers": 0, "cloud_or_model_requests": 0, "alert_type": alerts[0].Type, "selected_alert_id": alerts[0].ID, "generated_source": emitted.Source})
	fmt.Println("Actual 1-SEC auth module generated event+alert; exported through upstream JSON marshaling; no enforcement handler installed.")
}
