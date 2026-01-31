-- Test unqualified table names (should trigger the alias bug)
CREATE PROCEDURE [dbo].[GetAccountUnqualified]
    @AccountId INT
AS
BEGIN
    SET NOCOUNT ON;

    SELECT
        A.Id AS AccountId,
        A.AccountNumber,
        T.Name AS TagName
    FROM
        Account A
        INNER JOIN AccountTag AT ON AT.AccountId = A.Id
        INNER JOIN Tag T ON T.Id = AT.TagId
    WHERE
        A.Id = @AccountId;
END;
GO
