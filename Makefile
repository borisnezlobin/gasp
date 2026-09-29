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

.PHONY: help build run dmg dmg-local icon ios-core ios-project ios-sim ios-phone

help:
	@echo "make build       Build the desktop app, signed so the Keychain remembers it"
	@echo "make run         Build and open the desktop app on your last vault"
	@echo "make dmg         Signed, notarized Gasp.dmg for other Macs (target/package/)"
	@echo "make dmg-local   Unsigned Gasp.dmg that only runs on this Mac"
	@echo "make icon        Rebuild AppIcon.icns from the whale render"
	@echo "make ios-sim     Build the iPhone app and run it in the simulator (no window)"
	@echo "make ios-phone   Build the iPhone app and install it on the plugged-in iPhone"

build:
	cargo build --release -p gasp-desktop
	codesign --force --sign "$(DEV_SIGNING_ID)" --identifier $(BUNDLE_ID) target/release/gasp

run: build
	./target/release/gasp

dmg:
	DEVELOPER_ID="$(DEVELOPER_ID)" NOTARY_PROFILE="$(NOTARY_PROFILE)" scripts/package-macos.sh

dmg-local:
	DEVELOPER_ID= NOTARY_PROFILE= scripts/package-macos.sh

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
		-allowProvisioningUpdates -allowProvisioningDeviceRegistration build
	xcrun devicectl device install app --device $(IPHONE) $(IOS_BUILD)/Build/Products/Release-iphoneos/Gasp.app
	xcrun devicectl device process launch --device $(IPHONE) com.borisnezlobin.gasp
