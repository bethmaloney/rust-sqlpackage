-- View with unqualified table names (should trigger the alias bug)
CREATE VIEW [dbo].[AccountSummaryUnqualified]
AS
SELECT
    A.Id,
    A.AccountNumber,
    A.Status,
    A.CreatedOn,
    T.Name AS TagName
FROM
    Account A
    INNER JOIN AccountTag AT ON AT.AccountId = A.Id
    INNER JOIN Tag T ON T.Id = AT.TagId;
GO
