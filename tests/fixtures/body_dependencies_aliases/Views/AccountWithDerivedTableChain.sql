-- View with chained derived tables (nested FROM clauses)
-- Tests alias resolution through multiple derived table levels
CREATE VIEW [dbo].[AccountWithDerivedTableChain]
AS
SELECT
    OuterDerived.Id,
    OuterDerived.AccountNumber,
    OuterDerived.TagCount
FROM (
    SELECT
        MiddleDerived.Id,
        MiddleDerived.AccountNumber,
        MiddleDerived.TagCount
    FROM (
        SELECT
            A.Id,
            A.AccountNumber,
            (
                SELECT COUNT(*)
                FROM [dbo].[AccountTag] [AT1]
                WHERE [AT1].AccountId = A.Id
            ) AS TagCount
        FROM [dbo].[Account] A
    ) MiddleDerived
    WHERE MiddleDerived.TagCount > 0
) OuterDerived;
GO
