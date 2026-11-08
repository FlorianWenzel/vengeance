package core

import "testing"

func TestCompareVersions(t *testing.T) {
	cases := []struct {
		a, b string
		want int
	}{
		{"1.2.3", "1.2.3", 0},
		{"1.2.3", "1.2.10", -1},
		{"1.10.0", "1.9.9", 1},
		{"v1.2.0", "1.2.0", 0},
		{"1.2.0-rc1", "1.2.0", -1},
		{"3.3.2-r0", "3.3.2-r1", -1},
		{"3.3.10-r0", "3.3.2-r9", 1},
		{"2.36-9+deb12u7", "2.36-9+deb12u10", -1},
		{"1:2.3", "2.4", -1},
		{"1.2", "1.2.1", -1},
	}
	for _, c := range cases {
		if got := CompareVersions(c.a, c.b); got != c.want {
			t.Errorf("CompareVersions(%q,%q)=%d want %d", c.a, c.b, got, c.want)
		}
	}
}

func TestPickFix(t *testing.T) {
	cases := []struct{ installed, fixed, want string }{
		{"2.0.1", "2.5.0", "2.5.0"},
		{"4.17.20", "4.17.21, 5.0.1", "4.17.21"},
		{"5.0.0", "4.17.21, 5.0.1", "5.0.1"},
		{"1.0.0", "", ""},
		{"2.0.0", "1.9.0", ""},
		{"0.9.0", "1.2.0, 1.1.0", "1.1.0"},
	}
	for _, c := range cases {
		if got := PickFix(c.installed, c.fixed); got != c.want {
			t.Errorf("PickFix(%q,%q)=%q want %q", c.installed, c.fixed, got, c.want)
		}
	}
}

func TestPriority(t *testing.T) {
	cases := []struct {
		in   PriorityInput
		want int
	}{
		{PriorityInput{Kind: "vuln", Severity: "low", HasFix: true, KEV: true}, 1},
		{PriorityInput{Kind: "vuln", Severity: "critical", HasFix: true, AssetTags: []string{"internet-facing"}}, 1},
		{PriorityInput{Kind: "vuln", Severity: "critical", HasFix: true}, 2},
		{PriorityInput{Kind: "vuln", Severity: "high", HasFix: true}, 2},
		{PriorityInput{Kind: "vuln", Severity: "medium", HasFix: true}, 3},
		{PriorityInput{Kind: "vuln", Severity: "critical", HasFix: false}, 4},
		{PriorityInput{Kind: "secret", Severity: "high"}, 1},
		{PriorityInput{Kind: "secret", Severity: "low"}, 3},
		{PriorityInput{Kind: "misconfig", Severity: "high"}, 2},
		{PriorityInput{Kind: "misconfig", Severity: "low"}, 3},
	}
	for i, c := range cases {
		if got := Priority(c.in); got != c.want {
			t.Errorf("case %d: Priority(%+v)=%d want %d", i, c.in, got, c.want)
		}
	}
}

func TestCommand(t *testing.T) {
	if got := Command("npm", "multer", "2.5.0"); got != "npm install multer@2.5.0" {
		t.Error(got)
	}
	if got := Command("gomod", "golang.org/x/net", "0.38.0"); got != "go get golang.org/x/net@v0.38.0 && go mod tidy" {
		t.Error(got)
	}
}
