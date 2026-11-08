package core

import (
	"strconv"
	"strings"
	"unicode"
)

// CompareVersions compares two loosely semver-like version strings segment by segment.
// Numeric segments compare as numbers, others lexically; a pre-release suffix sorts before
// the release ("1.2.0-rc1" < "1.2.0"). Good enough for npm, Go, pip, cargo, apk and dpkg in
// the common case; it is used to pick a fix target, not to decide whether something is vulnerable.
func CompareVersions(a, b string) int {
	pa, preA := splitVersion(a)
	pb, preB := splitVersion(b)
	for i := 0; i < len(pa) || i < len(pb); i++ {
		var x, y string
		if i < len(pa) {
			x = pa[i]
		}
		if i < len(pb) {
			y = pb[i]
		}
		if c := compareSeg(x, y); c != 0 {
			return c
		}
	}
	switch {
	case preA == preB:
		return 0
	case preA == "":
		return 1
	case preB == "":
		return -1
	}
	return strings.Compare(preA, preB)
}

func splitVersion(v string) ([]string, string) {
	v = strings.TrimSpace(v)
	v = strings.TrimPrefix(v, "v")
	if i := strings.IndexByte(v, ':'); i >= 0 && i < 3 { // dpkg epoch
		v = v[i+1:]
	}
	pre := ""
	if i := strings.IndexAny(v, "-+~"); i >= 0 {
		// apk "-r1" and dpkg "-1ubuntu1" are revisions, not pre-releases: keep them as segments.
		rest := v[i+1:]
		if v[i] == '-' && len(rest) > 0 && (rest[0] == 'r' && isDigits(rest[1:]) || unicode.IsDigit(rune(rest[0]))) {
			v = v[:i] + "." + rest
		} else {
			pre = rest
			v = v[:i]
		}
	}
	return strings.FieldsFunc(v, func(r rune) bool { return r == '.' || r == '-' || r == '_' }), pre
}

func isDigits(s string) bool {
	if s == "" {
		return false
	}
	for _, r := range s {
		if !unicode.IsDigit(r) {
			return false
		}
	}
	return true
}

// compareSeg compares two segments run by run: digit runs numerically, other runs lexically
// (rpmvercmp style), so "9+deb12u7" < "9+deb12u10".
func compareSeg(a, b string) int {
	if a == "" || b == "" {
		switch {
		case a == b:
			return 0
		case a == "":
			return -1
		}
		return 1
	}
	ra, rb := runs(a), runs(b)
	for i := 0; i < len(ra) && i < len(rb); i++ {
		x, y := ra[i], rb[i]
		nx, ex := strconv.ParseUint(x, 10, 64)
		ny, ey := strconv.ParseUint(y, 10, 64)
		if ex == nil && ey == nil {
			if nx != ny {
				if nx < ny {
					return -1
				}
				return 1
			}
			continue
		}
		if c := strings.Compare(x, y); c != 0 {
			return c
		}
	}
	switch {
	case len(ra) < len(rb):
		return -1
	case len(ra) > len(rb):
		return 1
	}
	return 0
}

func runs(s string) []string {
	var out []string
	start := 0
	for i := 1; i <= len(s); i++ {
		if i == len(s) || unicode.IsDigit(rune(s[i])) != unicode.IsDigit(rune(s[i-1])) {
			out = append(out, s[start:i])
			start = i
		}
	}
	return out
}

// PickFix chooses the version to upgrade to from a scanner's fixed-version field, which can
// list several (", "-separated) for different release lines. It returns the lowest listed
// version that is above installed and on the same major line if one exists, else the lowest
// above installed, else "".
func PickFix(installed, fixed string) string {
	var cands []string
	for _, f := range strings.Split(fixed, ",") {
		f = strings.TrimSpace(f)
		if f == "" {
			continue
		}
		if installed == "" || CompareVersions(f, installed) > 0 {
			cands = append(cands, f)
		}
	}
	if len(cands) == 0 {
		return ""
	}
	best := ""
	major := firstSeg(installed)
	for _, c := range cands {
		if major != "" && firstSeg(c) == major && (best == "" || CompareVersions(c, best) < 0) {
			best = c
		}
	}
	if best != "" {
		return best
	}
	best = cands[0]
	for _, c := range cands[1:] {
		if CompareVersions(c, best) < 0 {
			best = c
		}
	}
	return best
}

// MaxVersion returns the higher version.
func MaxVersion(a, b string) string {
	if a == "" {
		return b
	}
	if b == "" || CompareVersions(a, b) >= 0 {
		return a
	}
	return b
}

func firstSeg(v string) string {
	p, _ := splitVersion(v)
	if len(p) == 0 {
		return ""
	}
	return p[0]
}

// Command returns a copy-ready upgrade command for an ecosystem as Trivy names it.
func Command(ecosystem, pkg, version string) string {
	switch ecosystem {
	case "npm", "node-pkg":
		return "npm install " + pkg + "@" + version
	case "yarn":
		return "yarn up " + pkg + "@" + version
	case "pnpm":
		return "pnpm update " + pkg + "@" + version
	case "gomod", "gobinary":
		if pkg == "stdlib" {
			return "upgrade the Go toolchain to " + strings.TrimPrefix(version, "v") + " and rebuild"
		}
		v := version
		if !strings.HasPrefix(v, "v") {
			v = "v" + v
		}
		return "go get " + pkg + "@" + v + " && go mod tidy"
	case "pip", "pipenv", "poetry", "python-pkg", "uv":
		return "pip install '" + pkg + ">=" + version + "'"
	case "cargo", "rust-binary":
		return "cargo update -p " + pkg + " --precise " + version
	case "composer":
		return "composer require " + pkg + ":^" + version
	case "bundler", "gemspec":
		return "bundle update " + pkg
	case "nuget", "dotnet-core":
		return "dotnet add package " + pkg + " --version " + version
	case "jar", "pom", "gradle":
		return "bump " + pkg + " to " + version + " in your build file"
	case "alpine", "wolfi", "chainguard":
		return "apk upgrade --no-cache " + pkg + "  # or rebuild on a newer base image"
	case "debian", "ubuntu":
		return "apt-get install --only-upgrade " + pkg + "  # or rebuild on a newer base image"
	case "redhat", "centos", "rocky", "alma", "amazon", "oracle", "fedora", "cbl-mariner", "azurelinux":
		return "dnf upgrade " + pkg + "  # or rebuild on a newer base image"
	case "suse", "opensuse", "opensuse.leap", "opensuse.tumbleweed", "sles":
		return "zypper update " + pkg
	}
	return "upgrade " + pkg + " to " + version
}
