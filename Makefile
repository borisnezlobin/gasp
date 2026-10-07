# Everyday commands. `make help` lists them.

DEVELOPER_ID ?= Developer ID Application: Zihao Yan (K2MB68Z582)
NOTARY_PROFILE ?= editor-notary
ICON_PYTHON ?= $(firstword $(wildcard $(HOME)/miniforge3/bin/python3) python3)
IPHONE ?= $(shell xcrun devicectl list devices 2>/dev/null | grep iPhone | grep -v unavailable | grep -oE '[0-9A-F]{8}-([0-9A-F]{4}-){3}[0-9A-F]{12}' | head -1)
IOS_BUILD := target/ios-derived
# Everyday builds are signed with a stable identity, so the Keychain's "Always Allow"
# survives rebuilds instead of asking again after every one.
DEV_SIGNING_ID ?= Apple Development: boris.nezlobin@gmail.com (XY4F56L5NQ)
BUNDLE_ID := com.borisnezlobin.gasp
# The paid team signs for a year; the free personal team (3N3S6262N2) works until Xcode has
# the paid account, but its installs stop opening after 7 days.
IOS_TEAM ?= K2MB68Z582

.PHONY: help build run dmg dmg-local dmg-background notarize icon ios-core ios-project ios-sim ios-phone ios-upload snapshot tidy

help:
	@echo "make build       Build the desktop app, signed so the Keychain remembers it"
	@echo "make run         Build and open the desktop app on your last vault"
	@echo "make snapshot    Run a snapshot script on a copy of VAULT, no window shown:"
	@echo "                 make snapshot SCRIPT=steps.txt VAULT=path [OUT=dir] [OPEN=note]"
	@echo "make dmg         Signed, notarized Gasp.dmg for other Macs (target/package/)"
	@echo "make dmg-local   Unsigned Gasp.dmg that only runs on this Mac"
	@echo "make notarize    Notarize the Gasp.dmg already built (needs the Mac unlocked)"
	@echo "make tidy        Delete old builds Cargo left in target/ (runs after build and dmg)"
	@echo "make icon        Rebuild AppIcon.icns from the whale render"
	@echo "make dmg-background  Redraw the install window's background art"
	@echo "make ios-sim     Build the iPhone app and run it in the simulator (no window)"
	@echo "make ios-phone   Build the iPhone app and install it on the plugged-in iPhone"
	@echo "make ios-upload  Archive the iPhone app and upload it to App Store Connect for TestFlight"

build:
	cargo build --release -p gasp-desktop
	codesign --force --sign "$(DEV_SIGNING_ID)" --identifier $(BUNDLE_ID) target/release/gasp
	@scripts/prune-target.py

run: build
	./target/release/gasp

# Follows SCRIPT in the whole window on a copy of VAULT and writes its PNGs to
# OUT. The debug build leaves the signed release binary alone.
OUT ?= target/snapshots
snapshot:
	@test -n "$(SCRIPT)" -a -n "$(VAULT)" || { echo "usage: make snapshot SCRIPT=steps.txt VAULT=path [OUT=dir] [OPEN=note]"; exit 2; }
	cargo build -p gasp-desktop
	./target/debug/gasp --snapshot --vault "$(VAULT)" --script "$(SCRIPT)" --out "$(OUT)" $(if $(OPEN),--open "$(OPEN)")

dmg:
	DEVELOPER_ID="$(DEVELOPER_ID)" NOTARY_PROFILE="$(NOTARY_PROFILE)" scripts/package-macos.sh
	@scripts/prune-target.py

tidy:
	@scripts/prune-target.py

VERSION := $(shell sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)

notarize:
	xcrun notarytool submit target/package/Gasp-$(VERSION).dmg --keychain-profile "$(NOTARY_PROFILE)" --wait
	xcrun stapler staple target/package/Gasp-$(VERSION).dmg
	spctl -a -vv -t install target/package/Gasp-$(VERSION).dmg

dmg-local:
	DEVELOPER_ID= NOTARY_PROFILE= scripts/package-macos.sh

dmg-background:
	"$(ICON_PYTHON)" scripts/dmg-background.py

icon:
	PYTHON="$(ICON_PYTHON)" apps/desktop/assets/icon/build_icon.sh

ios-core:
	apps/ios/scripts/build-core.sh

ios-project: ios-core
	cd apps/ios && xcodegen generate

ios-sim: ios-project
	apps/ios/scripts/run-simulator.sh

ios-phone: ios-project
	@test -n "$(IPHONE)" || { echo "No unlocked iPhone found. Plug it in and unlock it."; exit 1; }
	xcodebuild -project apps/ios/Gasp.xcodeproj -scheme Gasp -configuration Release \
		-destination 'generic/platform=iOS' -derivedDataPath $(IOS_BUILD) \
		-allowProvisioningUpdates -allowProvisioningDeviceRegistration DEVELOPMENT_TEAM=$(IOS_TEAM) build
	xcrun devicectl device install app --device $(IPHONE) $(IOS_BUILD)/Build/Products/Release-iphoneos/Gasp.app
	xcrun devicectl device process launch --device $(IPHONE) com.borisnezlobin.gasp

ios-upload:
	apps/ios/scripts/upload-testflight.sh
	@scripts/prune-target.py
