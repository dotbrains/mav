use super::text_renderer::ClusterAnalyzer;

#[test]
fn test_cluster_map() {
    let cluster_map = [0];
    let mut analyzer = ClusterAnalyzer::new(&cluster_map, 1);
    let next = analyzer.next();
    assert_eq!(next, Some((1, 1)));
    let next = analyzer.next();
    assert_eq!(next, None);

    let cluster_map = [0, 1, 2];
    let mut analyzer = ClusterAnalyzer::new(&cluster_map, 3);
    let next = analyzer.next();
    assert_eq!(next, Some((1, 1)));
    let next = analyzer.next();
    assert_eq!(next, Some((1, 1)));
    let next = analyzer.next();
    assert_eq!(next, Some((1, 1)));
    let next = analyzer.next();
    assert_eq!(next, None);
    // 👨‍👩‍👧‍👦👩‍💻
    let cluster_map = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 4, 4, 4];
    let mut analyzer = ClusterAnalyzer::new(&cluster_map, 5);
    let next = analyzer.next();
    assert_eq!(next, Some((11, 4)));
    let next = analyzer.next();
    assert_eq!(next, Some((5, 1)));
    let next = analyzer.next();
    assert_eq!(next, None);
    // 👩‍💻
    let cluster_map = [0, 0, 0, 0, 0];
    let mut analyzer = ClusterAnalyzer::new(&cluster_map, 1);
    let next = analyzer.next();
    assert_eq!(next, Some((5, 1)));
    let next = analyzer.next();
    assert_eq!(next, None);
}
