# WorldMap: Überarbeitung und Prüfung

Stand: 12. September 2026. Änderungen sind lokal implementiert, ohne Veröffentlichung.

## Oberfläche und Karten

Neue Atlas-Oberfläche mit ruhiger Farbpalette, besserer Typografie, gruppierten und filterbaren Ebenen, Schnellansichten, Legenden, Statusanzeigen und mobilen Panels. Dunkle und helle CARTO-Karten, Globus, Regionsnavigation, Tastatur-Ortssuche und besser platzierte Detailansichten. Die Karten-Engine wird separat geladen; die Oberfläche erscheint bereits währenddessen. MapLibre 6 und deck.gl 9.4 sind über die passende MapLibre-Integration verbunden.

## Geschwindigkeit

- Der eigene Layer-Aufbau läuft bei Änderungen statt in einer endlosen Animationsschleife. Unveränderte Layer werden wiederverwendet. Ein Test mit 10.000 Invalidierungen bestätigt einen gebündelten Frame; im Leerlauf wird kein weiterer Frame dieses Schedulers angefordert.
- Schiffs-Updates werden gebündelt; unnötige Koordinatenkopien entfallen. Polling vermeidet überlappende Anfragen und pausiert ausgewählte Abfragen in unsichtbaren Tabs.
- Statische Flughafen- und Hafendaten werden einmal serialisiert und komprimiert. Antworten verwenden gemeinsam genutzte Byte-Puffer und HTTP-Caching.
- Flughafen-Antwort: 1.077.348 Bytes unkomprimiert, 194.624 Bytes gzip, rund 82 % weniger Transfer. Zehn lokale Anfragen gegen den Rust-Debugserver: gzip-Median nach Vorberechnung 0,33 ms gegenüber 84,31 ms bei Komprimierung je Anfrage. Diese Messung betrifft diesen Endpunkt, nicht die gesamte App oder Internet-Latenz.
- Der direkt eingebundene JavaScript-Einstieg plus Laufzeit umfasst etwa 79 KB gzip. Große Kartenmodule werden asynchron geladen. Die gesamte Karten-Engine bleibt groß; Vite meldet weiterhin einen Chunk über 1 MB unkomprimiert.
- Cache-Miss-Sperren vermeiden parallele identische externe Abrufe und mehrfache Token-Erneuerungen. Wetterabfragen sind räumlich verteilt und pro Aktualisierung begrenzt.

## Behobene Fehler

- Verspätete Schiffs-Snapshots überschreiben keine neueren Live-Daten; Verbindungen stoppen und verbinden kontrolliert neu.
- Historische Wiedergabe verwendet vorhandene Zeitstempel, verwirft überholte Anfragen und bewahrt Datensätze mit unbekanntem Kurs oder Tempo.
- Wechselnde Flugauswahlen überschreiben einander nicht mehr; fehlende Koordinaten erzeugen keine falsche Strecke durch 0/0.
- Veraltete Flugpositionen werden als zwischengespeichert gekennzeichnet. Ein ausgefallener Feed ohne Cache liefert einen Fehler statt eines falschen Datenformats.
- SQLite-Schreibvorgänge rollen bei Fehlern zurück; alte Schiffspositionen bleiben nicht unbegrenzt im Live-Speicher.
- Wetterdaten und Windrichtung stimmen mit den abgefragten Feldern überein; erfundene Nullwerte für nicht gelieferte Wettergrößen entfallen.
- Fehlgeschlagene Speichern-, Löschen- und Bestätigen-Anfragen erscheinen als Fehler und werden nicht als Erfolg dargestellt.
- Watchlist-Parameter werden korrekt als Objekt übertragen; ältere doppelt kodierte Einträge bleiben lesbar. Das Formular erfasst MMSI beziehungsweise Koordinaten für die Ereignisprüfung.
- Ungültige Wetterkoordinaten, Ereignisparameter und Tile-Koordinaten werden abgefangen. Tile-Gzip wird anhand der tatsächlichen Daten erkannt.
- Native Kartenebenen werden nach Stilwechsel wiederhergestellt. Kartenhöhe bleibt auch nach asynchronem CSS-Laden korrekt. Nicht vorhandene lokale Tile-Dateien verursachen keine nutzlosen Tile-Anfragen.
- Abhängigkeiten aktualisiert und ungenutzte Pakete entfernt; npm audit meldet aktuell 0 bekannte Schwachstellen.

## Validierung

- Frontend: TypeScript/Vite-Build, ESLint und 17 Tests erfolgreich.
- Backend: Build und 7 Rust-Tests erfolgreich.
- 17 lesende HTTP-Smoke-Checks erfolgreich, einschließlich Datensätze, History, Gzip, Cache-Header und ungültiger Eingaben.
- Browser: Produktionsbuild geladen; dunkle/helle Karte, Globus, Ebenen, Ortssuche nach Berlin mit korrektem Kartenziel, Historienbedienung und Watchlist-Felder geprüft. Mobile Ansicht bei 390 × 844 im Entwicklungsbuild geprüft.
- CI führt Frontend-Lint, Tests und Build sowie Backend-Tests aus.

Reproduzieren:

```sh
cd frontend
npm ci
npm run lint
npm test
npm run build
cd ../backend
cargo test
cargo run
# In einem zweiten Terminal im Projektverzeichnis:
python3 scripts/test_smoke.py
```

## Verbleibende Grenzen

Im lokalen Datenverzeichnis fehlen die MBTiles für Pipelines, Stromnetz und Hochspannungsleitungen. Der TomTom-Schlüssel ist nicht konfiguriert. Diese Daten können durch UI-Änderungen nicht ersetzt werden; die App zeigt den Zustand nun an. Live-Feeds bleiben von Verfügbarkeit und Limits der Anbieter abhängig. Langzeitlast, sämtliche externen Datenfälle und jedes Endgerät wurden nicht vollständig geprüft. Vorhandene Rust-Warnungen zu ungenutztem Code bleiben bestehen. Eine Garantie, dass keinerlei weitere Bugs existieren, ergibt sich aus diesen Prüfungen nicht.
