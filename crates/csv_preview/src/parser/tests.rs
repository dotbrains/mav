use super::*;

#[test]
fn test_csv_parsing_basic() {
    let csv_data = "Name,Age,City\nJohn,30,New York\nJane,25,Los Angeles";
    let parsed = TableLikeContent::from_str(csv_data.to_string());

    assert_eq!(parsed.headers.cols(), 3);
    assert_eq!(parsed.headers[0].display_value().unwrap().as_ref(), "Name");
    assert_eq!(parsed.headers[1].display_value().unwrap().as_ref(), "Age");
    assert_eq!(parsed.headers[2].display_value().unwrap().as_ref(), "City");

    assert_eq!(parsed.rows.len(), 2);
    assert_eq!(parsed.rows[0][0].display_value().unwrap().as_ref(), "John");
    assert_eq!(parsed.rows[0][1].display_value().unwrap().as_ref(), "30");
    assert_eq!(
        parsed.rows[0][2].display_value().unwrap().as_ref(),
        "New York"
    );
}

#[test]
fn test_csv_parsing_with_quotes() {
    let csv_data = r#"Name,Description
"John Doe","A person with ""special"" characters"
Jane,"Simple name""#;
    let parsed = TableLikeContent::from_str(csv_data.to_string());

    assert_eq!(parsed.headers.cols(), 2);
    assert_eq!(parsed.rows.len(), 2);
    assert_eq!(
        parsed.rows[0][1].display_value().unwrap().as_ref(),
        r#"A person with "special" characters"#
    );
}

#[test]
fn test_csv_parsing_with_newlines_in_quotes() {
    let csv_data = "Name,Description,Status\n\"John\nDoe\",\"A person with\nmultiple lines\",Active\n\"Jane Smith\",\"Simple\",\"Also\nActive\"";
    let parsed = TableLikeContent::from_str(csv_data.to_string());

    assert_eq!(parsed.headers.cols(), 3);
    assert_eq!(parsed.headers[0].display_value().unwrap().as_ref(), "Name");
    assert_eq!(
        parsed.headers[1].display_value().unwrap().as_ref(),
        "Description"
    );
    assert_eq!(
        parsed.headers[2].display_value().unwrap().as_ref(),
        "Status"
    );

    assert_eq!(parsed.rows.len(), 2);
    assert_eq!(
        parsed.rows[0][0].display_value().unwrap().as_ref(),
        "John\nDoe"
    );
    assert_eq!(
        parsed.rows[0][1].display_value().unwrap().as_ref(),
        "A person with\nmultiple lines"
    );
    assert_eq!(
        parsed.rows[0][2].display_value().unwrap().as_ref(),
        "Active"
    );

    assert_eq!(
        parsed.rows[1][0].display_value().unwrap().as_ref(),
        "Jane Smith"
    );
    assert_eq!(
        parsed.rows[1][1].display_value().unwrap().as_ref(),
        "Simple"
    );
    assert_eq!(
        parsed.rows[1][2].display_value().unwrap().as_ref(),
        "Also\nActive"
    );

    assert_eq!(parsed.line_numbers.len(), 2);
    match &parsed.line_numbers[0] {
        LineNumber::LineRange(start, end) => {
            assert_eq!(start, &2);
            assert_eq!(end, &4);
        }
        _ => panic!("Expected LineRange for multiline row"),
    }
    match &parsed.line_numbers[1] {
        LineNumber::LineRange(start, end) => {
            assert_eq!(start, &5);
            assert_eq!(end, &6);
        }
        _ => panic!("Expected LineRange for second multiline row"),
    }
}

#[test]
fn test_empty_csv() {
    let parsed = TableLikeContent::from_str("".to_string());
    assert_eq!(parsed.headers.cols(), 0);
    assert!(parsed.rows.is_empty());
}

#[test]
fn test_csv_parsing_quote_offset_handling() {
    let csv_data = r#"first,"se,cond",third"#;
    let (parsed_cells, _) = parse_csv_with_positions(csv_data);

    assert_eq!(parsed_cells.len(), 1);
    assert_eq!(parsed_cells[0].len(), 3);

    let (content1, range1) = &parsed_cells[0][0];
    assert_eq!(content1.as_ref(), "first");
    assert_eq!(*range1, 0..5);

    let (content2, range2) = &parsed_cells[0][1];
    assert_eq!(content2.as_ref(), "se,cond");
    assert_eq!(*range2, 6..15);

    let (content3, range3) = &parsed_cells[0][2];
    assert_eq!(content3.as_ref(), "third");
    assert_eq!(*range3, 16..21);
}

#[test]
fn test_csv_parsing_complex_quotes() {
    let csv_data = r#"id,"name with spaces","description, with commas",status
1,"John Doe","A person with ""quotes"" and, commas",active
2,"Jane Smith","Simple description",inactive"#;
    let (parsed_cells, _) = parse_csv_with_positions(csv_data);

    assert_eq!(parsed_cells.len(), 3);

    let header_row = &parsed_cells[0];
    assert_eq!(header_row.len(), 4);

    assert_eq!(header_row[0].0.as_ref(), "id");
    assert_eq!(header_row[0].1, 0..2);

    assert_eq!(header_row[1].0.as_ref(), "name with spaces");
    assert_eq!(header_row[1].1, 3..21);

    assert_eq!(header_row[2].0.as_ref(), "description, with commas");
    assert_eq!(header_row[2].1, 22..48);

    assert_eq!(header_row[3].0.as_ref(), "status");
    assert_eq!(header_row[3].1, 49..55);

    let first_row = &parsed_cells[1];
    assert_eq!(first_row.len(), 4);

    assert_eq!(first_row[0].0.as_ref(), "1");
    assert_eq!(first_row[0].1, 56..57);

    assert_eq!(first_row[1].0.as_ref(), "John Doe");
    assert_eq!(first_row[1].1, 58..68);

    assert_eq!(
        first_row[2].0.as_ref(),
        r#"A person with "quotes" and, commas"#
    );
    assert_eq!(first_row[2].1, 69..107);

    assert_eq!(first_row[3].0.as_ref(), "active");
    assert_eq!(first_row[3].1, 108..114);
}
