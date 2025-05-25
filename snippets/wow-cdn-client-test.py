
def retrBytes(url):
    import urllib.request
    return urllib.request.urlopen(url).read()

def retr(url):
    return retrBytes(url).decode("utf-8")

cdns = retr("http://us.patch.battle.net:1119/wow/cdns")
versions = retr("http://us.patch.battle.net:1119/wow/versions")
print(cdns, versions)

def formatHexKey(key):
    return key[0:2] + "/" + key[2:4] + "/" + key

for line in versions.splitlines():
    parts = line.split("|")
    if parts[0] != "us":
        continue
    buildConfig = parts[1]
    cdnConfig = parts[2]

for line in cdns.splitlines():
    parts = line.split("|")
    if parts[0] != "us":
        continue
    cdn = parts[2].split(" ")[2]
    path = "http://" + cdn + "/" + parts[1] + "/"

    buildCfgUrl = path + "config/" + formatHexKey(buildConfig)
    cdnCfgUrl = path + "config/" + formatHexKey(cdnConfig)

cdnCfg = retr(cdnCfgUrl)
for line in cdnCfg.splitlines():
    parts = list(line.split(" = "))
    if not parts:
        continue
    if parts[0] == "archives":
        archives = parts[1].split(" ")

print(f"Found {len(archives)} archives")

index1 = retrBytes(path + "data/" + formatHexKey(archives[0]) + ".index")
print("Archive 0 hash: " + archives[0])

import pathlib
pathlib.Path("archive0").write_bytes(index1)