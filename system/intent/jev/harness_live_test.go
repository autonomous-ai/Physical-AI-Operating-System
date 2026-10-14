package jev

import (
	"context"
	"encoding/json"
	"os"
	"strings"
	"testing"
	"time"
)

// Opt-in session-ownership evaluation only: this never contacts Harness.
// Run the compiled test on a device with JEV_EVAL_CONFIG=/root/config/config.json.
func TestJevHarnessLiveOwnership(t *testing.T) {
	path := os.Getenv("JEV_EVAL_CONFIG")
	if path == "" {
		t.Skip("set JEV_EVAL_CONFIG to explicitly enable live proxy evaluation")
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal("cannot read evaluation configuration")
	}
	var config struct {
		BaseURL string `json:"llm_base_url"`
		APIKey  string `json:"llm_api_key"`
	}
	if json.Unmarshal(raw, &config) != nil || config.BaseURL == "" || config.APIKey == "" {
		t.Fatal("evaluation requires proxy URL and key")
	}
	client := &jevClient{harness: true}
	for _, tc := range append(harnessLiveCases(), harnessHeldOutCases()...) {
		t.Run(tc.text, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), DefaultTimeout)
			defer cancel()
			started := time.Now()
			got, err := client.decide(ctx, strings.TrimRight(config.BaseURL, "/")+"/jev/decisions", config.APIKey, tc.text, tc.sessions)
			t.Logf("decision_ms=%d selected=%q expected=%q", time.Since(started).Milliseconds(), got.Intent, tc.want)
			if err != nil {
				t.Fatal("live decision failed; credentials and response omitted")
			}
			if got.Intent != tc.want {
				t.Errorf("selected %q, want %q", got.Intent, tc.want)
			}
		})
	}
}

type harnessLiveCase struct {
	text, want string // want "" means abstain and keep the main agent's target
	sessions   []Candidate
}

// harnessSession mirrors the facts harness_selection.go sends for one agent.
func harnessSession(id, name, recap, workspace, runtime string) Candidate {
	facts, _ := json.Marshal(map[string]any{
		"name": name, "recap": recap, "workspace": map[string]string{"path": workspace},
		"runtime": runtime, "state": "idle",
	})
	return Candidate{ID: id, Description: string(facts)}
}

// Clear owners, new projects, ambiguity and shared-application traps share one
// corpus so a speed change cannot silently trade precision for recall.
func harnessLiveCases() []harnessLiveCase {
	design := []Candidate{
		harnessSession("session_0", "Airplane", "Modeled a twin-engine airplane fuselage and wings in Blender.", "/Users/me/Projects/Airplane", "claude-code"),
		harnessSession("session_1", "House", "Built a two-storey house with a front garden in Blender.", "/Users/me/Projects/House", "claude-code"),
		harnessSession("session_2", "Shop web", "Added a checkout page to the Next.js online shop.", "/Users/me/Projects/shop-web", "codex"),
	}
	work := []Candidate{
		harnessSession("session_0", "Q3 report", "Drafted the Q3 sales report in Google Docs with a revenue chart.", "/Users/me/Documents/Reports", "claude-code"),
		harnessSession("session_1", "Go API", "Fixed the login rate limiter in the Go API service.", "/Users/me/code/api", "codex"),
		harnessSession("session_2", "iOS app", "Added dark mode to the SwiftUI iPhone app.", "/Users/me/code/ios-app", "claude-code"),
		harnessSession("session_3", "Orders ETL", "Wrote the Python job that loads orders into BigQuery every night.", "/Users/me/code/etl", "opencode"),
	}
	twins := []Candidate{
		harnessSession("session_0", "Scene A", "Started a new Blender scene.", "/Users/me/Projects/SceneA", "claude-code"),
		harnessSession("session_1", "Scene B", "Started a new Blender scene.", "/Users/me/Projects/SceneB", "claude-code"),
	}
	return []harnessLiveCase{
		{"Add a few trees to the garden of the house.", "session_1", design},
		{"Make the airplane wings a bit longer.", "session_0", design},
		{"Paint the house roof dark red.", "session_1", design},
		{"Add a propeller to each engine of the airplane.", "session_0", design},
		{"Add a coupon field to the checkout page.", "session_2", design},
		{"Fix the checkout button on the online shop, it does nothing.", "session_2", design},
		{"Thêm vài cây vào khu vườn của ngôi nhà.", "session_1", design},
		{"Làm cánh máy bay dài hơn một chút.", "session_0", design},
		{"Not the airplane. Add the trees to the house project.\nOriginal Task: Add trees to the garden.", "session_1", design},
		{"Add trees to the garden.\nOriginal Task: Built a two-storey house with a front garden.", "session_1", design},
		{"Create a new Blender model of a sailing boat.", "", design},
		{"Open Blender for me.", "", design},
		{"Add more detail to the Blender model.", "", design},
		{"Write a poem about autumn.", "", design},
		{"Start a brand new project: a recipe website.", "", design},
		{"Continue the last task.", "", design},
		{"Add a garden to the airplane hangar scene.", "", design},
		{"Add a revenue-by-region table to the Q3 sales report.", "session_0", work},
		{"Raise the login rate limit to ten attempts per minute in the API.", "session_1", work},
		{"The dark mode in the iPhone app has unreadable grey text, fix it.", "session_2", work},
		{"Make the nightly orders job also load refunds into BigQuery.", "session_3", work},
		{"Thêm bảng doanh thu theo vùng vào báo cáo Q3.", "session_0", work},
		{"Sửa lỗi chữ xám khó đọc trong dark mode của app iPhone.", "session_2", work},
		{"The orders ETL failed last night, check why.", "session_3", work},
		{"Add a unit test for the login rate limiter.", "session_1", work},
		{"Draft the Q4 marketing plan.", "", work},
		{"Fix the bug.", "", work},
		{"Build an Android version of a weather app.", "", work},
		// The admin page is part of the Go API project; relabeled after the first run.
		{"Add dark mode to the Go API's admin web page.", "session_1", work},
		{"Check the code.", "", work},
		{"Add a lamp to the Blender scene.", "", twins},
		{"Render the Blender scene.", "", twins},
		{"In Scene B, add a lamp next to the sofa.", "session_1", twins},
		{"Add a camera to the scene in /Users/me/Projects/SceneA.", "session_0", twins},
	}
}

// harnessHeldOutCases were written after the fit threshold was chosen on
// harnessLiveCases and are scored only to check it.
func harnessHeldOutCases() []harnessLiveCase {
	media := []Candidate{
		harnessSession("session_0", "Podcast edit", "Cut episode 12 of the cooking podcast in Audacity and normalized loudness.", "/Users/me/Podcast", "claude-code"),
		harnessSession("session_1", "Album cover", "Designed the album cover for Night Drive in Figma.", "/Users/me/Design/NightDrive", "codex"),
		harnessSession("session_2", "Band website", "Built the tour dates page for the band's Astro website.", "/Users/me/code/band-site", "claude-code"),
	}
	research := []Candidate{
		harnessSession("session_0", "Thesis", "Wrote chapter 2 of the thesis on coral reef bleaching in LaTeX.", "/Users/me/Thesis", "claude-code"),
		harnessSession("session_1", "Grant proposal", "Drafted the budget section of the NSF grant proposal in Google Docs.", "/Users/me/Documents/Grant", "codex"),
		harnessSession("session_2", "Lab data", "Cleaned the reef temperature CSV files in a Jupyter notebook.", "/Users/me/lab-data", "opencode"),
	}
	django := []Candidate{
		harnessSession("session_0", "Shop backend", "Added Stripe webhooks to the shop's Django backend.", "/Users/me/code/shop-api", "codex"),
		harnessSession("session_1", "Blog backend", "Added an RSS feed to the blog's Django backend.", "/Users/me/code/blog-api", "codex"),
	}
	cad := []Candidate{
		harnessSession("session_0", "CAD bracket", "Modeled a steel mounting bracket in FreeCAD.", "/Users/me/cad/bracket", "claude-code"),
	}
	return []harnessLiveCase{
		{"Remove the long pause at minute 14 of the cooking podcast.", "session_0", media},
		{"Make the album cover title bigger.", "session_1", media},
		{"Add the Berlin show on 3 March to the tour dates page.", "session_2", media},
		{"Thêm buổi diễn ở Hà Nội vào trang lịch lưu diễn.", "session_2", media},
		{"Design a poster for the Berlin show.", "", media},
		{"Edit the video for episode 13.", "", media},
		{"Make it louder.", "", media},
		{"Add three more references to chapter 2 of the thesis.", "session_0", research},
		{"Lower the travel budget in the grant proposal by ten percent.", "session_1", research},
		{"Plot the reef temperature data by month.", "session_2", research},
		{"Sửa lỗi chính tả trong chương 2 của luận văn.", "session_0", research},
		{"Write a new paper about coral reef bleaching.", "", research},
		{"Summarize the latest news on coral reefs.", "", research},
		{"Update the document.", "", research},
		{"Retry the failed Stripe webhooks in the shop backend.", "session_0", django},
		{"Add an RSS category filter to the blog.", "session_1", django},
		{"Add Stripe payments to the blog backend.", "session_1", django},
		{"No, not the shop. Add the RSS change to the blog project.\nOriginal Task: Add an RSS category filter.", "session_1", django},
		{"Upgrade Django in the backend.", "", django},
		{"Fix the Django backend tests.", "", django},
		{"Add two mounting holes to the bracket.", "session_0", cad},
		{"Make the bracket 2 mm thicker.", "session_0", cad},
		{"Kiểm tra xem giá đỡ có chịu được 50 kg không.", "session_0", cad},
		{"Model a new steel hinge in FreeCAD.", "", cad},
		{"Open FreeCAD.", "", cad},
	}
}

func TestJevHarnessLiveCorpusLabelsOfferedSessions(t *testing.T) {
	for _, tc := range append(harnessLiveCases(), harnessHeldOutCases()...) {
		if err := validateJevCandidates(tc.sessions); err != nil {
			t.Fatalf("%q: %v", tc.text, err)
		}
		found := tc.want == ""
		for _, s := range tc.sessions {
			found = found || s.ID == tc.want
		}
		if !found {
			t.Fatalf("%q: label %q is not an offered session", tc.text, tc.want)
		}
	}
}
