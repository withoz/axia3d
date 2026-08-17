p='CLAUDE.md'
s=open(p,encoding='utf-8').read()
old = s[s.index('그런 차집합은 **한 점에서 꼬집힌 두 영역**이지 한 폴리곤이 아닙니다.'):s.index('**L-105-9**')]
new = '''그런 차집합은 **한 점에서 꼬집힌 두 영역**이지 한 폴리곤이 아니다. 거절이 첫 답
이었고 겹침은 남았다. **조각들이 진짜 답이다** — P 를 두 번 지나는 고리는 첫 P 에서
둘째 P 까지의 고리 + 둘째 P 에서 돌아 첫 P 까지의 고리이고, 넓이가 정확히 더해진다
(실측: 8정점 고리 → [2500, 2500] 합 5000).

```
was   FaceId(37)  31정점, 중복 2   → 교차점 0, lens 0      (읽을 수 없음)
그다음 거절만      겹침 1건 남음     → 교차점 4, lens 8정점   (읽히지만 안 나뉨)
is    조각 둘로 세움                 → 위반 0, 면 32 → 33    (나뉨)
```

**수리를 없애는 것은 답이 아니다 (실측)**: 두 호출부를 다 끄면 **3741 통과 / 8 실패**
이고 퍼즈 세션 10 이 op 11 에서 다시 깨진다. `blast_radius` ·
`a_second_rect_nested_in_the_first_still_takes_its_piece` ·
`the_same_draw_over_a_one_shot_face_now_resolves_too` 등이 수리에 기대고 있다.
문제는 수리가 도는 것이 아니라 **한 조각으로 돌려주는 것**이었다.

'''
s=s.replace(old,new,1)
s=s.replace('''**L-105-9** 차집합 결과가 **떨어진 위치에서** 자기를 만나면 거절한다
(`polygon_difference_by_clip`). 연속 중복 제거만으로는 부족하다 — 재구축의
`add_vertex` dedup 이 그걸 하나의 VertId 로 접는다.''','''**L-105-9** 차집합은 **조각들**을 돌려준다 (`polygon_difference_by_clip` →
`Vec<Vec<_>>`). 떨어진 위치에서 자기를 만나는 고리는 그 점에서 정확히 갈라진다
(`split_ring_at_self_touches`). 연속 중복 제거만으로는 부족하다 — 재구축의
`add_vertex` dedup 이 그걸 하나의 VertId 로 접는다.''',1)
s=s.replace('''**L-105-10** 자기접촉 루프를 가진 면은 **어떤 계측기도 읽을 수 없다.** 두 계측기가
한 쌍에 대해 정반대를 말하면 폴리곤 자체를 먼저 의심한다.''','''**L-105-10** 자기접촉 루프를 가진 면은 **어떤 계측기도 읽을 수 없다.** 두 계측기가
한 쌍에 대해 정반대를 말하면 폴리곤 자체를 먼저 의심한다.
**L-105-11** 수리(`subtract_double_covered_faces`)는 **없애면 안 된다** — 실측 8건이
그것에 기댄다. 잘못된 결과를 만들면 고칠 곳은 수리를 끄는 자리가 아니라 **무엇을
돌려주는지** 다.''',1)
s=s.replace('''| 수리가 **정점을 두 번 지나는 면**을 만듦 | `polygon_difference_by_clip` | 떨어진 재방문을 거르지 않았다 |''','''| 수리가 **정점을 두 번 지나는 면**을 만듦 | `polygon_difference_by_clip` | 꼬집힌 결과를 **한 조각으로** 돌려줬다 |''',1)
open(p,'w',encoding='utf-8').write(s)
print('LOCKED #105 updated')
