-- Procedure with UPDATE...FROM containing aliases
-- Tests alias resolution in UPDATE statements with FROM clause
CREATE PROCEDURE [dbo].[UpdateAccountWithFrom]
    @TagName NVARCHAR(100)
AS
BEGIN
    SET NOCOUNT ON;

    UPDATE A
    SET A.Status = 'Tagged',
        A.ModifiedOn = GETDATE()
    FROM [dbo].[Account] A
    INNER JOIN [dbo].[AccountTag] [AT1] ON [AT1].AccountId = A.Id
    INNER JOIN [dbo].[Tag] [T1] ON [T1].Id = [AT1].TagId
    WHERE [T1].[Name] = @TagName;
END;
GO
