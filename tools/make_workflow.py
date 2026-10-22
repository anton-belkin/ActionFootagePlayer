#!/usr/bin/env python3
# make_workflow.py <out.workflow> <menu title> <icon> <shell command>
#
# Writes a Finder Quick Action (an Automator service for movie files) that runs a shell command with the
# selected files as arguments ("$@"), like one saved from Automator with a single Run Shell Script action.

import plistlib, sys, uuid, os, shutil

out, title, icon, command = sys.argv[1:5]

action = {
    "AMAccepts": { "Container": "List", "Optional": True, "Types": ["com.apple.cocoa.string"] },
    "AMActionVersion": "2.0.3",
    "AMApplication": ["Automator"],
    "AMParameterProperties": { k: {} for k in ["COMMAND_STRING", "CheckedForUserDefaultShell", "inputMethod", "shell", "source"] },
    "AMProvides": { "Container": "List", "Types": ["com.apple.cocoa.string"] },
    "ActionBundlePath": "/System/Library/Automator/Run Shell Script.action",
    "ActionName": "Run Shell Script",
    "ActionParameters": {
        "COMMAND_STRING": command,
        "CheckedForUserDefaultShell": True,
        "inputMethod": 1,  # as arguments
        "shell": "/bin/zsh",
        "source": "",
    },
    "BundleIdentifier": "com.apple.RunShellScript",
    "CFBundleVersion": "2.0.3",
    "CanShowSelectedItemsWhenRun": False,
    "CanShowWhenRun": True,
    "Category": ["AMCategoryUtilities"],
    "Class Name": "RunShellScriptAction",
    "InputUUID": str(uuid.uuid4()).upper(),
    "Keywords": ["Shell", "Script", "Command", "Run", "Unix"],
    "OutputUUID": str(uuid.uuid4()).upper(),
    "UUID": str(uuid.uuid4()).upper(),
    "UnlocalizedApplications": ["Automator"],
    "arguments": {
        "0": { "default value": 0, "name": "inputMethod", "required": "0", "type": "0", "uuid": "0" },
        "1": { "default value": False, "name": "CheckedForUserDefaultShell", "required": "0", "type": "0", "uuid": "1" },
        "2": { "default value": "", "name": "source", "required": "0", "type": "0", "uuid": "2" },
        "3": { "default value": "", "name": "COMMAND_STRING", "required": "0", "type": "0", "uuid": "3" },
        "4": { "default value": "/bin/sh", "name": "shell", "required": "0", "type": "0", "uuid": "4" },
    },
    "isViewVisible": 1,
    "location": "309.000000:305.000000",
    "nibPath": "/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib",
    "conversionLabel": 0,
}

document = {
    "AMApplicationBuild": "512",
    "AMApplicationVersion": "2.10",
    "AMDocumentVersion": "2",
    "actions": [{ "action": action }],
    "connectors": {},
    "workflowMetaData": {
        "applicationBundleIDsByPath": { "/System/Library/CoreServices/Finder.app": "com.apple.finder" },
        "applicationPaths": ["/System/Library/CoreServices/Finder.app"],
        "inputTypeIdentifier": "com.apple.Automator.fileSystemObject.movie",
        "outputTypeIdentifier": "com.apple.Automator.nothing",
        "presentationMode": 15,
        "processesInput": False,
        "serviceApplicationBundleID": "com.apple.finder",
        "serviceApplicationPath": "/System/Library/CoreServices/Finder.app",
        "serviceInputTypeIdentifier": "com.apple.Automator.fileSystemObject.movie",
        "serviceOutputTypeIdentifier": "com.apple.Automator.nothing",
        "serviceProcessesInput": False,
        "systemImageName": icon,
        "useAutomaticInputType": False,
        "workflowTypeIdentifier": "com.apple.Automator.servicesMenu",
    },
}

info = {
    "NSServices": [{
        "NSIconName": icon,
        "NSMenuItem": { "default": title },
        "NSMessage": "runWorkflowAsService",
        "NSRequiredContext": { "NSApplicationIdentifier": "com.apple.finder" },
        "NSSendFileTypes": ["public.movie"],
    }]
}

if os.path.exists(out):
    shutil.rmtree(out)
os.makedirs(os.path.join(out, "Contents"))
with open(os.path.join(out, "Contents", "document.wflow"), "wb") as f:
    plistlib.dump(document, f)
with open(os.path.join(out, "Contents", "Info.plist"), "wb") as f:
    plistlib.dump(info, f)
